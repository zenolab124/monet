use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::engines::claude::diagnostics;
use crate::engines::claude::protocol::{self, ControlOutcome, LiveSignal, ReverseRequest};
use crate::engines::claude::remote::launch::{self as remote_launch, RemoteLaunch};
use crate::models::ContentBlock;
use crate::permission::PermissionService;
use crate::session_capabilities::{
    needs_restart as capability_needs_restart, SessionCapabilityBundle, SessionCapabilityId,
};

/// 长活进程会话实例（进程跨轮复用，不杀不重启）
struct SessionProcess {
    child: Child,
    stdin: ChildStdin,
    request_counter: u64,
    channel: Option<String>,
    effort: Option<String>,
    /// spawn 时的快速模式覆盖(None = 跟随 CLI/项目配置)
    fast_mode: Option<bool>,
    /// 进程当前生效的模型(spawn --model / set_model 下达值;None = CLI 默认)。
    /// 复用判定用:意图回落 None 而进程钉着旧值时必须重启,set_model 无法"切回默认"
    model: Option<String>,
    /// Monet 显式下发的权限覆盖；None 表示由 CLI 从磁盘配置继承。
    /// control protocol 无法撤销覆盖，Some → None 时必须重启进程。
    permission_mode: Option<String>,
    /// spawn 时的顾问开关(advisor 经 --settings 注入,变更只能重启生效)
    advisor: bool,
    /// spawn 时的 Chrome 集成开关(--chrome 是启动参数,变更只能重启生效)
    chrome: bool,
    /// spawn 时的用户自定义 CLI 参数原始串(逃生舱,变更只能重启生效)
    extra_args: String,
    /// Monet 专属启动能力的稳定指纹；变化只能通过重启 Claude 进程生效
    capability_fingerprint: String,
    /// 进程启动墙钟时刻(epoch ms)。异步账本的进程代际判据:启动早于本时刻的
    /// 无终态任务属于已死的前代进程,终态通知永远不会来,不得再判"进行中"
    started_at_ms: i64,
    /// 已发出、等待 CLI 判决的权限模式切换（request_id → 请求）
    pending_permission: HashMap<String, PendingPermission>,
    /// 本进程是否已上报过 system/init 中的插件/MCP 加载问题（每进程只提示一次）
    init_reported: bool,
    /// 运行中插入、尚未到达终态的用户消息（uuid → 状态）
    pending_injections: HashMap<String, PendingInjection>,
    /// 等待 CLI 回包的同步控制请求（request_id → 回包投递口）
    pending_controls: HashMap<String, std::sync::mpsc::Sender<Result<Value, String>>>,
    /// 远端会话：本机 child 是 ssh（回环时是 sh），CLI 跑在远端主机上
    remote: Option<RemoteProcess>,
}

#[derive(Clone)]
struct RemoteProcess {
    launch: RemoteLaunch,
    /// 启动脚本报告的远端 claude pid；关闭、断线与下次连接时据此精确清理
    pid: Option<u32>,
}

struct PendingInjection {
    /// 已收到 CLI 的 command_lifecycle 回报（任意状态）
    acknowledged: bool,
}

struct PendingPermission {
    requested: String,
    /// 超时已向前端报失败；迟到的成功判决仍会生效并重新同步界面
    timed_out: bool,
}

/// ssh 连接失败或中断时的退出码
const SSH_CONNECTION_LOST: i32 = 255;

/// 关闭远端会话后等多久再确认远端进程已退出（关闭 stdin 后 CLI 通常 1～2 秒内退出）
const REMOTE_EXIT_GRACE: Duration = Duration::from_secs(3);

/// 权限模式切换等待 CLI 判决的时限
const PERMISSION_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);

/// 运行中插入的消息等待 CLI 回报 queued 的时限
const INJECTION_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// 文件回退等待 CLI 回包的时限（真正回退要逐个恢复文件，留足余量）
const REWIND_FILES_TIMEOUT: Duration = Duration::from_secs(30);

/// 仅用于 SessionProcess 尚未接管 Child 的启动阶段错误路径。
fn terminate_and_reap_startup_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// 自定义参数黑名单:会打爆 Monet↔CLI 通信的协议级参数(会话身份/流式格式/权限桥/渠道注入)。
/// 冲突级参数(--model/--effort/--chrome 等)刻意放行——追加在 Monet 参数之后,CLI 后者
/// 覆盖语义让用户能强行改写,这正是逃生舱的意义
const EXTRA_ARGS_DENYLIST: &[&str] = &[
    "-p",
    "--print",
    "-c",
    "--continue",
    "--output-format",
    "--input-format",
    "--session-id",
    "--resume",
    "--fork-session",
    "--mcp-config",
    "--permission-prompt-tool",
    "--settings",
];

/// 解析会话自定义 CLI 参数:shell 式分词(引号值支持),黑名单项连其值一并剔除。
/// 解析失败(未闭合引号等)返回空——宁可不加参数也不给 CLI 传半截
fn parse_extra_args(raw: &str) -> Vec<String> {
    let tokens = shell_words::split(raw).unwrap_or_default();
    let mut out = Vec::new();
    let mut skip_value = false;
    for t in tokens {
        if skip_value {
            skip_value = false;
            // 非 - 开头 = 黑名单 flag 的值,一并剔除;- 开头则不是值,落回正常处理
            if !t.starts_with('-') {
                continue;
            }
        }
        let flag_part = t.split('=').next().unwrap_or(&t);
        if EXTRA_ARGS_DENYLIST.contains(&flag_part) {
            // --flag value 形式:标记跳过下一个值 token(--flag=value 形式无值可跳)
            skip_value = !t.contains('=');
            continue;
        }
        out.push(t);
    }
    out
}

#[cfg(test)]
mod extra_args_tests {
    use super::parse_extra_args;

    #[test]
    fn keeps_normal_args() {
        assert_eq!(
            parse_extra_args("--betas foo --fallback-model opus"),
            vec!["--betas", "foo", "--fallback-model", "opus"]
        );
    }

    #[test]
    fn strips_denied_flag_with_value() {
        assert_eq!(
            parse_extra_args("--output-format json --betas foo"),
            vec!["--betas", "foo"]
        );
    }

    #[test]
    fn strips_denied_eq_form() {
        assert_eq!(
            parse_extra_args("--session-id=abc --betas foo"),
            vec!["--betas", "foo"]
        );
    }

    #[test]
    fn boolean_denied_flag_does_not_eat_next_flag() {
        // --print 是布尔 flag,其后的 --betas 不能被当作值误剔
        assert_eq!(
            parse_extra_args("--print --betas foo"),
            vec!["--betas", "foo"]
        );
    }

    #[test]
    fn quoted_values_survive() {
        assert_eq!(
            parse_extra_args(r#"--append-system-prompt "hello world""#),
            vec!["--append-system-prompt", "hello world"]
        );
    }

    #[test]
    fn unclosed_quote_yields_empty() {
        assert!(parse_extra_args(r#"--betas "unclosed"#).is_empty());
    }
}

/// 活跃的长活进程表（sessionId → SessionProcess）
static ACTIVE_PROCESSES: Mutex<Option<HashMap<String, Arc<Mutex<SessionProcess>>>>> =
    Mutex::new(None);

/// 获取 Monet 为指定会话持有的长活进程 PID（check_session_running 排除自有进程用）
pub fn get_own_pid(session_id: &str) -> Option<u32> {
    ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id))
        .map(|sp| sp.lock().unwrap().child.id())
}

/// 自有长活进程的启动时刻(epoch ms)；无进程返回 None
/// 进程键是否属于本机实例：本机会话的键就是 CLI 会话 uuid；远端主机会话的键是界面结构化 id
/// （必然含 `.`，uuid 不会），这类进程不能在本机启动，也不归本机运行时跟踪
pub fn is_local_session_key(session_id: &str) -> bool {
    !session_id.contains('.')
}

/// Monet 为该会话在远端启动的 claude pid（远端运行态检测据此排除自己）
pub fn get_own_remote_pid(session_id: &str) -> Option<u32> {
    let arc = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())?;
    let sp = arc.lock().unwrap();
    sp.remote.as_ref().and_then(|remote| remote.pid)
}

pub fn get_own_started_at_ms(session_id: &str) -> Option<i64> {
    ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id))
        .map(|sp| sp.lock().unwrap().started_at_ms)
}

/// 前端接收的流式事件（全部携带 session_id，前端按会话分发）
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 助手消息内容更新（终态快照，作为字符级 delta 的兜底校正）
    AssistantMessage {
        session_id: String,
        message_id: String,
        content: Vec<ContentBlock>,
        /// 本轮实际运行的模型(message.model 真值)
        model: Option<String>,
        /// 该 API message 的 token 用量(快照自带,message 完成即有)——
        /// 前端借此在每段回复结束时即时显示块级 usage,不必等整轮落账
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
    },
    /// 字符级增量到达——content_block_start：某个 index 上出现新块
    BlockStart {
        session_id: String,
        message_id: String,
        index: usize,
        content_block: ContentBlock,
        /// 本轮实际运行的模型(message_start 事件 message.model 真值,随首块带出)
        model: Option<String>,
    },
    /// 字符级增量到达——content_block_delta：某个 index 上的块字段增长
    /// delta 原样透传给前端，由前端按 delta.type 派发：
    /// - text_delta { type, text }
    /// - input_json_delta { type, partial_json }
    /// - thinking_delta { type, thinking }
    /// - signature_delta { type, signature }
    BlockDelta {
        session_id: String,
        message_id: String,
        index: usize,
        delta: Value,
    },
    /// 字符级增量到达——content_block_stop：某个 index 上的块结束
    BlockStop {
        session_id: String,
        message_id: String,
        index: usize,
    },
    /// 工具执行完成后的结果。CLI 将它作为 user message 发回，不能走
    /// Anthropic content_block_delta，因此单独作为语义事件转发。
    ToolResults {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        content: Vec<ContentBlock>,
    },
    /// 流式完成（携带 modelUsage 真值：上下文容量 + token 用量）
    Result {
        session_id: String,
        text: String,
        cost_usd: f64,
        context_window: Option<u64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    /// 错误
    Error { session_id: String, message: String },
}

/// 流式链路累计计数，供开发者性能 HUD 对比入口事件率与实际 IPC 批次率。
/// 计数只增不减；前端按相邻采样差值换算每秒速率。
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct StreamPerfCounters {
    pub input_events: u64,
    pub emitted_batches: u64,
    pub emitted_events: u64,
    pub merged_deltas: u64,
}

static STREAM_INPUT_EVENTS: AtomicU64 = AtomicU64::new(0);
static STREAM_EMITTED_BATCHES: AtomicU64 = AtomicU64::new(0);
static STREAM_EMITTED_EVENTS: AtomicU64 = AtomicU64::new(0);
static STREAM_MERGED_DELTAS: AtomicU64 = AtomicU64::new(0);
static STREAM_EVENT_PUMP: OnceLock<SyncSender<StreamDispatch>> = OnceLock::new();

#[derive(Clone)]
pub enum StreamingObserverEvent {
    Stream(StreamEvent),
    Signal { name: String, payload: Value },
}

pub type StreamingObserver = Arc<dyn Fn(StreamingObserverEvent) + Send + Sync>;

static STREAM_OBSERVERS: Mutex<Vec<(u64, StreamingObserver)>> = Mutex::new(Vec::new());
static NEXT_STREAM_OBSERVER_ID: AtomicU64 = AtomicU64::new(1);

pub struct StreamingObserverHandle(u64);

impl Drop for StreamingObserverHandle {
    fn drop(&mut self) {
        STREAM_OBSERVERS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|(id, _)| *id != self.0);
    }
}

pub fn subscribe_observer(observer: StreamingObserver) -> StreamingObserverHandle {
    let id = NEXT_STREAM_OBSERVER_ID.fetch_add(1, Ordering::Relaxed);
    STREAM_OBSERVERS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push((id, observer));
    StreamingObserverHandle(id)
}

fn notify_observers(event: StreamingObserverEvent) {
    let observers: Vec<_> = STREAM_OBSERVERS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|(_, observer)| Arc::clone(observer))
        .collect();
    for observer in observers {
        observer(event.clone());
    }
}

const STREAM_BATCH_WINDOW: Duration = Duration::from_millis(32);
const STREAM_BATCH_CAPACITY: usize = 256;
const STREAM_QUEUE_CAPACITY: usize = 8192;

#[derive(Debug)]
enum StreamDispatch {
    Event(StreamEvent),
    Signal { name: &'static str, payload: Value },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeltaKey {
    message_id: String,
    index: usize,
    delta_type: String,
}

#[derive(Default)]
struct StreamBatch {
    events: Vec<StreamEvent>,
    /// 每个会话只允许与它自己的最后一个 delta 合并；其他会话的事件不构成屏障。
    last_delta_by_session: HashMap<String, (DeltaKey, usize)>,
}

impl StreamBatch {
    fn push(&mut self, event: StreamEvent) -> bool {
        let session_id = event.session_id().to_string();
        if let Some(key) = event.delta_key() {
            if let Some((last_key, event_index)) = self.last_delta_by_session.get(&session_id) {
                if *last_key == key && merge_delta(&mut self.events[*event_index], &event) {
                    return true;
                }
            }
            let event_index = self.events.len();
            self.events.push(event);
            self.last_delta_by_session
                .insert(session_id, (key, event_index));
        } else {
            // 同会话的语义事件是严格屏障；不同会话仍可继续各自合并。
            self.last_delta_by_session.remove(&session_id);
            self.events.push(event);
        }
        false
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn len(&self) -> usize {
        self.events.len()
    }
}

impl StreamEvent {
    pub(crate) fn session_id(&self) -> &str {
        match self {
            Self::AssistantMessage { session_id, .. }
            | Self::BlockStart { session_id, .. }
            | Self::BlockDelta { session_id, .. }
            | Self::BlockStop { session_id, .. }
            | Self::ToolResults { session_id, .. }
            | Self::Result { session_id, .. }
            | Self::Error { session_id, .. } => session_id,
        }
    }

    fn delta_key(&self) -> Option<DeltaKey> {
        let Self::BlockDelta {
            message_id,
            index,
            delta,
            ..
        } = self
        else {
            return None;
        };
        Some(DeltaKey {
            message_id: message_id.clone(),
            index: *index,
            delta_type: delta.get("type")?.as_str()?.to_string(),
        })
    }

    fn is_delta(&self) -> bool {
        matches!(self, Self::BlockDelta { .. })
    }
}

fn delta_text_field(delta: &Value) -> Option<(&'static str, &str)> {
    let field = match delta.get("type")?.as_str()? {
        "text_delta" => "text",
        "thinking_delta" => "thinking",
        "input_json_delta" => "partial_json",
        "signature_delta" => "signature",
        _ => return None,
    };
    Some((field, delta.get(field)?.as_str()?))
}

/// 合并同一会话、message、block、delta 类型的字符串负载。
fn merge_delta(target: &mut StreamEvent, incoming: &StreamEvent) -> bool {
    let (
        StreamEvent::BlockDelta {
            delta: target_delta,
            ..
        },
        StreamEvent::BlockDelta {
            delta: incoming_delta,
            ..
        },
    ) = (target, incoming)
    else {
        return false;
    };
    let Some((incoming_field, incoming_text)) = delta_text_field(incoming_delta) else {
        return false;
    };
    let Some((target_field, target_text)) = delta_text_field(target_delta) else {
        return false;
    };
    if target_field != incoming_field {
        return false;
    }
    let mut merged = String::with_capacity(target_text.len() + incoming_text.len());
    merged.push_str(target_text);
    merged.push_str(incoming_text);
    let Some(slot) = target_delta.get_mut(target_field) else {
        return false;
    };
    *slot = Value::String(merged);
    true
}

#[cfg(test)]
mod stream_batch_tests {
    use super::*;

    fn text_delta(session: &str, text: &str) -> StreamEvent {
        StreamEvent::BlockDelta {
            session_id: session.to_string(),
            message_id: format!("message-{session}"),
            index: 0,
            delta: json!({ "type": "text_delta", "text": text }),
        }
    }

    fn delta_text(event: &StreamEvent) -> &str {
        let StreamEvent::BlockDelta { delta, .. } = event else {
            panic!("expected block delta");
        };
        delta.get("text").and_then(Value::as_str).unwrap()
    }

    #[test]
    fn coalesces_interleaved_sessions_independently() {
        let mut batch = StreamBatch::default();
        let mut merged = 0;
        for _ in 0..100 {
            for sid in 0..7 {
                merged += usize::from(batch.push(text_delta(&format!("s{sid}"), "x")));
            }
        }

        assert_eq!(batch.len(), 7);
        assert_eq!(merged, 693);
        assert!(batch
            .events
            .iter()
            .all(|event| delta_text(event).len() == 100));
    }

    #[test]
    fn semantic_event_is_a_per_session_ordering_barrier() {
        let mut batch = StreamBatch::default();
        assert!(!batch.push(text_delta("s1", "a")));
        assert!(!batch.push(StreamEvent::BlockStop {
            session_id: "s1".to_string(),
            message_id: "message-s1".to_string(),
            index: 0,
        }));
        assert!(!batch.push(text_delta("s1", "b")));

        assert_eq!(batch.len(), 3);
        assert_eq!(delta_text(&batch.events[0]), "a");
        assert!(matches!(batch.events[1], StreamEvent::BlockStop { .. }));
        assert_eq!(delta_text(&batch.events[2]), "b");
    }

    #[test]
    fn preserves_signature_and_partial_json_chunks() {
        let mut batch = StreamBatch::default();
        for (delta_type, field, left, right) in [
            ("signature_delta", "signature", "sig-", "tail"),
            ("input_json_delta", "partial_json", "{\"a\":", "1}"),
        ] {
            let make = |text: &str| {
                let mut delta = json!({ "type": delta_type });
                delta[field] = json!(text);
                StreamEvent::BlockDelta {
                    session_id: delta_type.to_string(),
                    message_id: "m".to_string(),
                    index: 0,
                    delta,
                }
            };
            assert!(!batch.push(make(left)));
            assert!(batch.push(make(right)));
        }

        let serialized = serde_json::to_value(&batch.events).unwrap();
        assert_eq!(serialized[0]["delta"]["signature"], "sig-tail");
        assert_eq!(serialized[1]["delta"]["partial_json"], "{\"a\":1}");
    }
}

pub fn stream_perf_counters() -> StreamPerfCounters {
    StreamPerfCounters {
        input_events: STREAM_INPUT_EVENTS.load(Ordering::Relaxed),
        emitted_batches: STREAM_EMITTED_BATCHES.load(Ordering::Relaxed),
        emitted_events: STREAM_EMITTED_EVENTS.load(Ordering::Relaxed),
        merged_deltas: STREAM_MERGED_DELTAS.load(Ordering::Relaxed),
    }
}

fn emit_stream_batch(app: &AppHandle, batch: StreamBatch) {
    if batch.is_empty() {
        return;
    }
    STREAM_EMITTED_BATCHES.fetch_add(1, Ordering::Relaxed);
    STREAM_EMITTED_EVENTS.fetch_add(batch.len() as u64, Ordering::Relaxed);
    let _ = app.emit("stream-events", batch.events);
}

fn run_stream_event_pump(app: AppHandle, receiver: Receiver<StreamDispatch>) {
    while let Ok(first) = receiver.recv() {
        match first {
            StreamDispatch::Signal { name, payload } => {
                let _ = app.emit(name, payload);
            }
            StreamDispatch::Event(event) if !event.is_delta() => {
                let mut batch = StreamBatch::default();
                batch.push(event);
                emit_stream_batch(&app, batch);
            }
            StreamDispatch::Event(event) => {
                let mut batch = StreamBatch::default();
                batch.push(event);
                let deadline = Instant::now() + STREAM_BATCH_WINDOW;
                loop {
                    let now = Instant::now();
                    if now >= deadline || batch.len() >= STREAM_BATCH_CAPACITY {
                        emit_stream_batch(&app, batch);
                        break;
                    }
                    match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
                        Ok(StreamDispatch::Event(next)) => {
                            let barrier = !next.is_delta();
                            if batch.push(next) {
                                STREAM_MERGED_DELTAS.fetch_add(1, Ordering::Relaxed);
                            }
                            if barrier || batch.len() >= STREAM_BATCH_CAPACITY {
                                emit_stream_batch(&app, batch);
                                break;
                            }
                        }
                        Ok(StreamDispatch::Signal { name, payload }) => {
                            emit_stream_batch(&app, batch);
                            let _ = app.emit(name, payload);
                            break;
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            emit_stream_batch(&app, batch);
                            break;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            emit_stream_batch(&app, batch);
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn stream_event_sender(app: &AppHandle) -> &'static SyncSender<StreamDispatch> {
    STREAM_EVENT_PUMP.get_or_init(|| {
        let (sender, receiver) = sync_channel(STREAM_QUEUE_CAPACITY);
        let worker_app = app.clone();
        std::thread::Builder::new()
            .name("stream-event-pump".to_string())
            .spawn(move || run_stream_event_pump(worker_app, receiver))
            .expect("stream event pump thread must start");
        sender
    })
}

/// 所有 WebView 流式消息的统一入口：先进入有界队列，再由单一线程跨会话合批发射。
fn dispatch_stream_event(app: &AppHandle, event: StreamEvent) {
    STREAM_INPUT_EVENTS.fetch_add(1, Ordering::Relaxed);
    notify_observers(StreamingObserverEvent::Stream(event.clone()));
    let _ = stream_event_sender(app).send(StreamDispatch::Event(event));
}

/// 与流内容存在先后关系的生命周期信号也走同一队列，保证 result/error 已先抵达前端。
fn dispatch_stream_signal(app: &AppHandle, name: &'static str, payload: Value) {
    notify_observers(StreamingObserverEvent::Signal {
        name: name.to_string(),
        payload: payload.clone(),
    });
    let _ = stream_event_sender(app).send(StreamDispatch::Signal { name, payload });
}

/// 查找 claude 可执行文件路径（薄 wrapper，事实源在 claude_locator）。
/// 探测全败时保留 env fallback 兜底，行为不比旧版倒退；
/// 需要结构化错误的消费方（定时任务、设置页）直接用 claude_locator。
pub fn find_claude() -> (String, Vec<String>) {
    match crate::claude_locator::locate() {
        Ok(l) => (l.path.to_string_lossy().into_owned(), vec![]),
        Err(e) => {
            eprintln!("[claude-locator] {}", e);
            ("/usr/bin/env".to_string(), vec!["claude".to_string()])
        }
    }
}

// 增强 PATH 实现已抽至 path_env（runner 二进制经 #[path] 共享同一份源），
// 此处 re-export 保持既有调用点不变
pub use crate::path_env::enhanced_path;

/// 向长活进程 stdin 写入一行 JSON
fn write_stdin(stdin: &mut ChildStdin, msg: &Value) -> Result<(), String> {
    let line = serde_json::to_string(msg).map_err(|e| format!("JSON 序列化失败: {}", e))?;
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("stdin 写入失败: {}", e))
}

/// 向会话进程组发信号（macOS/Linux）。spawn 时已 process_group(0)，组 ID = CLI PID，
/// 组信号可连带回收 MCP 子进程树（monet-mcp/playwright/adb 等）。
/// 组信号失败时回退单 PID：兼容升级前 spawn 的存量进程（未自立进程组）。
#[cfg(unix)]
fn signal_session(pid: u32, sig: &str) {
    let group_ok = Command::new("kill")
        .args([sig, "--", &format!("-{}", pid)])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !group_ok {
        let _ = Command::new("kill").args([sig, &pid.to_string()]).output();
    }
}

/// Windows 无信号语义：TERM/KILL 一律 taskkill /T /F 整树强杀。
/// CLI 的 JSONL 逐行落盘，强杀不丢会话数据（与 close_all_sessions 注释同一依据）；
/// 已死进程上的第二次调用报 not found 被吞，与 unix 二段式击杀兼容。
#[cfg(windows)]
fn signal_session(pid: u32, _sig: &str) {
    use crate::proc_ext::HideConsole;
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .hide_console()
        .output();
}

/// 断链探测：同 id 的 jsonl 是否存在于其他 cwd 编码目录（如 EnterWorktree 迁走的）。
/// 命中说明"这不是新会话，是历史不在预期位置"——静默 --session-id 会以空历史
/// 重生同名会话，复刻「会话被覆盖」事故。
fn find_session_elsewhere(session_id: &str, expected: &Path) -> Option<PathBuf> {
    let root = crate::config::projects_dir();
    let name = format!("{}.jsonl", session_id);
    for entry in std::fs::read_dir(&root).ok()?.filter_map(|e| e.ok()) {
        let candidate = entry.path().join(&name);
        if candidate != expected && candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn normalized_override(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

pub(crate) fn fast_mode_unavailable_error(message: &str) -> bool {
    let text = message.to_ascii_lowercase();
    (text.contains("fast mode") || text.contains("fast-mode") || text.contains("fast_mode"))
        && [
            "unavailable",
            "not available",
            "not enabled",
            "not eligible",
            "disabled",
            "requires",
            "rejected",
            "exhausted",
            "limit reached",
        ]
        .iter()
        .any(|reason| text.contains(reason))
}

fn permission_change_requires_restart(current: Option<&str>, requested: Option<&str>) -> bool {
    current.is_some() && normalized_override(requested).is_none()
}

fn append_explicit_run_args(
    args: &mut Vec<String>,
    model: Option<&str>,
    effort: Option<&str>,
    permission_mode: Option<&str>,
) {
    if let Some(model) = normalized_override(model) {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    if let Some(effort) = normalized_override(effort).filter(|value| *value != "ultracode") {
        args.extend(["--effort".to_string(), effort.to_string()]);
    }
    if let Some(mode) = normalized_override(permission_mode) {
        args.extend(["--permission-mode".to_string(), mode.to_string()]);
    }
}

#[cfg(test)]
mod explicit_run_args_tests {
    use super::{
        append_explicit_run_args, fast_mode_unavailable_error, permission_change_requires_restart,
    };

    #[test]
    fn only_cli_uuids_are_local_process_keys() {
        assert!(super::is_local_session_key(
            "0b5c7e2a-1111-4222-8333-444455556666"
        ));
        assert!(!super::is_local_session_key(
            "sr1.Y2xhdWRlLWNvZGU.c3NoLWRldmJveA.MGI1Yw"
        ));
    }

    #[test]
    fn empty_overrides_add_no_cli_flags() {
        let mut args = vec!["--verbose".to_string()];
        append_explicit_run_args(&mut args, None, None, None);
        assert_eq!(args, vec!["--verbose"]);
    }

    #[test]
    fn adds_only_explicit_overrides() {
        let mut args = Vec::new();
        append_explicit_run_args(&mut args, Some("claude-opus-5"), Some("high"), Some("plan"));
        assert_eq!(
            args,
            vec![
                "--model",
                "claude-opus-5",
                "--effort",
                "high",
                "--permission-mode",
                "plan",
            ]
        );
    }

    #[test]
    fn ultracode_is_not_emitted_as_effort_flag() {
        let mut args = Vec::new();
        append_explicit_run_args(&mut args, None, Some("ultracode"), None);
        assert!(args.is_empty());
    }

    #[test]
    fn clearing_permission_override_requires_restart() {
        assert!(permission_change_requires_restart(Some("plan"), None));
        assert!(permission_change_requires_restart(Some("plan"), Some("")));
        assert!(!permission_change_requires_restart(
            Some("plan"),
            Some("default")
        ));
        assert!(!permission_change_requires_restart(None, None));
    }

    #[test]
    fn classifies_only_explicit_fast_mode_rejections() {
        assert!(fast_mode_unavailable_error(
            "Fast mode requires a paid subscription"
        ));
        assert!(fast_mode_unavailable_error(
            "Fast-mode unavailable during evaluation"
        ));
        assert!(!fast_mode_unavailable_error(
            "Server is temporarily unavailable"
        ));
    }
}

/// 打开会话进程：spawn 长活 CLI + 初始化握手 + 启动 stdout 读取线程
#[allow(clippy::too_many_arguments)] // 会话参数透传，拆结构体收益不大
fn open_session(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    model: Option<&str>,
    effort: Option<&str>,
    fast_mode: Option<bool>,
    channel: Option<&str>,
    advisor: bool,
    chrome: bool,
    fork_source: Option<&str>,
    extra_args: Option<&str>,
    permission_mode: Option<&str>,
    capability_bundle: &SessionCapabilityBundle,
    force_new: bool,
    remote: Option<&RemoteLaunch>,
) -> Result<(), String> {
    if remote.is_none() && !is_local_session_key(session_id) {
        return Err("REMOTE_SESSION_NOT_LOCAL".to_string());
    }
    // 远端会话的工作目录在远端；不存在时由启动脚本报告
    if remote.is_none() && !std::path::Path::new(cwd).is_dir() {
        return Err(format!("工作目录不存在: {}", cwd));
    }

    // 1. 权限服务（per-session 生命周期：open 时 start，进程退出时 stop）
    let permission_service = PermissionService::start(app.clone(), session_id)
        .map_err(|e| format!("权限服务启动失败：{}", e))?;
    let perm_endpoint = permission_service.endpoint().to_string();
    let perm_token = permission_service.token().to_string();

    let (mut command, resumed, channel_injection) = if let Some(launch) = remote {
        // 远端只带 CLI 原生参数：不注入渠道、本机 MCP 配置、--settings（顾问/快速模式/超档）、
        // Chrome 与 Monet 能力提示；权限经 stdio 控制请求走同一条 ssh 管道
        let mut args = if launch.resume {
            vec!["--resume".to_string(), launch.cli_session_id.clone()]
        } else {
            vec!["--session-id".to_string(), launch.cli_session_id.clone()]
        };
        args.extend(
            [
                "--output-format",
                "stream-json",
                "--input-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--thinking-display",
                "summarized",
                "--permission-prompt-tool",
                "stdio",
            ]
            .map(String::from),
        );
        append_explicit_run_args(&mut args, model, effort, permission_mode);
        if let Some(raw) = extra_args.filter(|s| !s.trim().is_empty()) {
            args.extend(parse_extra_args(raw));
        }
        remote_launch::cleanup_session_leftovers(launch);
        let script = remote_launch::launch_script_for(launch, &args)?;
        let mut command = crate::engines::claude::remote::executor::session_command(
            &launch.target,
            &script,
            launch.loopback,
        )
        .map_err(|e| format!("REMOTE_LAUNCH_FAILED:{e}"))?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        (command, launch.resume, None)
    } else {
        // 2. MCP 二进制
        let mcp_binary = match find_mcp_binary() {
            Some(p) => p,
            None => {
                PermissionService::stop_for(session_id);
                return Err("未找到 monet-mcp 二进制，无法启动权限服务".to_string());
            }
        };

        // 3. MCP 配置
        let mcp_config = json!({
            "mcpServers": {
                "monet": {
                    "type": "stdio",
                    "command": mcp_binary.to_string_lossy().into_owned(),
                    "args": [],
                    "env": {
                        "MONET_PERMISSION_ADDR": perm_endpoint.clone(),
                        "MONET_PERMISSION_TOKEN": perm_token
                    }
                }
            }
        })
        .to_string();

        // 4. CLI 参数（不带 --print，不传消息文本；加 --input-format stream-json）
        let (executable, prefix_args) = find_claude();
        let mut args = prefix_args;
        // cwd 先 canonicalize 再编码:CLI 落盘前对 cwd 做 realpath(mac 的 /tmp → /private/tmp,
        // Windows 的 junction/subst/8.3 短名/磁盘大小写校正),用原始 cwd 编码会对着
        // 不存在的目录做落盘判断,resume/fork 三态失灵。Windows canonicalize 产生的
        // \\?\ verbatim 前缀由 encode_project_dir 剥离,无需分平台
        let cwd_real = std::fs::canonicalize(cwd)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| cwd.to_string());
        // 编码必须与 CLI 同规则(非字母数字一律 →'-')：仅替换 '/' 会漏掉 '.'/'_'
        // 以及 Windows 的 ':'/'\\'，致 resume/fork 三态对着错误目录判定
        let project_dir = crate::config::projects_dir().join(crate::config::encode_project_dir(
            std::path::Path::new(&cwd_real),
        ));
        let session_file = project_dir.join(format!("{}.jsonl", session_id));
        // 会话身份参数三态:已落盘 → resume 自己;分叉意图且源已落盘 → CLI 原生 fork(resume 源 +
        // --fork-session + 指定新 ID,历史行 sessionId 由 CLI 重写,替代旧 fs::copy 仿造);
        // 否则全新会话。文件已存在时分叉意图残留无害,走正常 resume 忽略之;
        // 源未落盘(草稿源分叉)时意图同样无害,退化走新建分支——与前者对称
        let fork_src = fork_source
            .filter(|s| !s.is_empty())
            .filter(|src| project_dir.join(format!("{}.jsonl", src)).is_file());
        let (session_args, resumed) = if session_file.is_file() {
            (vec!["--resume".to_string(), session_id.to_string()], true)
        } else if let Some(src) = fork_src {
            (
                vec![
                    "--resume".to_string(),
                    src.to_string(),
                    "--fork-session".to_string(),
                    "--session-id".to_string(),
                    session_id.to_string(),
                ],
                true,
            )
        } else {
            if !force_new {
                // jsonl 不在预期位置——全局探测：是迁走了还是真的新会话？
                if let Some(actual) = find_session_elsewhere(session_id, &session_file) {
                    return Err(format!("SESSION_ELSEWHERE:{}", actual.to_string_lossy()));
                }
            }
            (
                vec!["--session-id".to_string(), session_id.to_string()],
                false,
            )
        };
        args.extend(session_args);
        args.extend([
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            // 真流式:CLI 2.1.177+ 默认只发 assistant 快照(整块,可能多次),该 flag 恢复
            // content_block_delta 字符级增量(实测 thinking/text 平均十几字符一个事件)。
            // 首字延迟从"整条消息生成完"缩到 API 首 token;快照仍会兜底到达,
            // 前端 feedSnapshotText 按已知长度差量对账,双路径幂等不双写
            "--include-partial-messages".to_string(),
            "--thinking-display".to_string(),
            "summarized".to_string(),
            "--mcp-config".to_string(),
            mcp_config,
            "--permission-prompt-tool".to_string(),
            "mcp__monet__approve_tool_use".to_string(),
        ]);

        // Chrome 集成(Claude in Chrome 扩展 + Native Messaging):按需开启,
        // 浏览器工具常驻吃上下文,默认不传;<2.1.211 的 CLI 在 Chrome 未运行时可能挂起
        if chrome {
            args.push("--chrome".to_string());
        }

        let ultracode = effort == Some("ultracode");

        // 5. 渠道/ultracode/顾问/快速模式注入（合成 --settings）
        let channel_opt = channel.filter(|s| !s.is_empty() && *s != crate::channels::OFFICIAL_ID);
        let channel_injection = match crate::channels::prepare_injection(
            channel_opt,
            session_id,
            ultracode,
            advisor,
            fast_mode,
        ) {
            Ok(Some(inj)) => {
                args.push("--settings".to_string());
                args.push(inj.settings_arg.clone());
                Some(inj)
            }
            Ok(None) => None,
            Err(e) => {
                PermissionService::stop_for(session_id);
                return Err(format!("会话配置加载失败:{}", e));
            }
        };

        append_explicit_run_args(&mut args, model, effort, permission_mode);
        if let Some(prompt) = capability_bundle.append_system_prompt() {
            args.push("--append-system-prompt".to_string());
            args.push(prompt.to_string());
        }
        // 用户自定义 CLI 参数(逃生舱):置于全部 Monet 参数之后,冲突项按 CLI 后者覆盖生效
        if let Some(raw) = extra_args.filter(|s| !s.trim().is_empty()) {
            args.extend(parse_extra_args(raw));
        }

        // 6. Spawn（stdin/stdout/stderr 全 piped）
        let mut command = Command::new(&executable);
        command
            .args(&args)
            .current_dir(cwd)
            .env("PATH", enhanced_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some((k, v)) = crate::config::claude_config_dir_env() {
            command.env(k, v);
        }
        // 文件快照：stream-json 模式默认不在用户消息处建快照，rewind_files 依赖它。
        // 用户自设 CLAUDE_CODE_DISABLE_FILE_CHECKPOINTING 时 CLI 仍以禁用为准
        command.env(protocol::FILE_CHECKPOINTING_ENV, "true");
        // CLI 自立进程组（组 ID = CLI PID）：MCP 子进程随组，signal_session 组信号可整树回收
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        // Windows 进程树由 taskkill /T 按父子关系回收，无需进程组；只抑制控制台闪窗
        #[cfg(windows)]
        {
            use crate::proc_ext::HideConsole;
            command.hide_console();
        }
        if let Some(inj) = &channel_injection {
            for key in &inj.clear_env {
                command.env_remove(key);
            }
            for (k, v) in &inj.env {
                command.env(k, v);
            }
        }
        (command, resumed, channel_injection)
    };
    let mut child = command.spawn().map_err(|e| {
        if let Some(inj) = &channel_injection {
            crate::channels::cleanup_runtime_file(&inj.runtime_path);
        }
        PermissionService::stop_for(session_id);
        format!("启动 claude 失败: {}", e)
    })?;

    // 7. Take handles
    let Some(mut stdin) = child.stdin.take() else {
        terminate_and_reap_startup_child(&mut child);
        return Err("无法获取 stdin".to_string());
    };
    let Some(stdout) = child.stdout.take() else {
        terminate_and_reap_startup_child(&mut child);
        return Err("无法获取 stdout".to_string());
    };
    let stderr = child.stderr.take();

    // 8. 初始化握手（没有这个握手，进程不处理任何消息）
    let init_msg = json!({
        "type": "control_request",
        "request_id": "init-1",
        "request": {"subtype": "initialize"}
    });
    if let Err(e) = write_stdin(&mut stdin, &init_msg) {
        PermissionService::stop_for(session_id);
        if let Some(inj) = &channel_injection {
            crate::channels::cleanup_runtime_file(&inj.runtime_path);
        }
        terminate_and_reap_startup_child(&mut child);
        return Err(format!("初始化握手写入失败: {}", e));
    }

    let mut reader = BufReader::new(stdout);
    let mut line_buf = String::new();
    let mut remote_pid: Option<u32> = None;
    loop {
        line_buf.clear();
        let bytes_read = match reader.read_line(&mut line_buf) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                PermissionService::stop_for(session_id);
                if let Some(inj) = &channel_injection {
                    crate::channels::cleanup_runtime_file(&inj.runtime_path);
                }
                terminate_and_reap_startup_child(&mut child);
                return Err(format!("初始化握手读取失败: {}", e));
            }
        };
        if bytes_read == 0 {
            PermissionService::stop_for(session_id);
            terminate_and_reap_startup_child(&mut child);
            let stderr_msg = stderr
                .map(|s| {
                    let mut r = BufReader::new(s);
                    let mut buf = String::new();
                    let _ = std::io::Read::read_to_string(&mut r, &mut buf);
                    buf
                })
                .unwrap_or_default();
            if let Some(inj) = &channel_injection {
                crate::channels::cleanup_runtime_file(&inj.runtime_path);
            }
            return Err(format!(
                "进程在初始化阶段退出{}",
                if stderr_msg.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", stderr_msg.trim())
                }
            ));
        }
        // 远端启动脚本在 exec 前报告 pid，工作目录不存在时报告错误并退出
        if remote.is_some() {
            match remote_launch::parse_marker(&line_buf) {
                Some(Ok(pid)) => {
                    remote_pid = Some(pid);
                    continue;
                }
                Some(Err(code)) => {
                    PermissionService::stop_for(session_id);
                    terminate_and_reap_startup_child(&mut child);
                    return Err(code.to_string());
                }
                None => {}
            }
        }
        if let Ok(v) = serde_json::from_str::<Value>(&line_buf) {
            let t = v.get("type").and_then(|t| t.as_str());
            if t == Some("control_response") {
                // 握手响应体是 CLI 自报的命令/agents/模型目录；解析失败回退磁盘扫描与内置列表
                if let Some((_, ControlOutcome::Success, Some(body))) =
                    protocol::control_response(&v)
                {
                    if let Some(catalog) = protocol::parse_initialize_response(body) {
                        if let Some(version) = catalog.cli_version.as_deref() {
                            diagnostics::note_cli_version(version);
                        }
                        // 模型目录按实例存：远端会话记在远端实例名下
                        let instance = match remote {
                            Some(launch) => Some(launch.instance.clone()),
                            None => crate::engines::claude::default_instance().ok(),
                        };
                        if let Some(instance) = instance.as_ref() {
                            diagnostics::store_models(instance, &catalog.models);
                        }
                        let _ = app.emit(
                            "session-catalog",
                            json!({ "session_id": session_id, "engine": instance, "catalog": catalog }),
                        );
                    } else {
                        eprintln!("[long-lived] 握手响应结构不符，回退本地命令与模型列表");
                    }
                }
                break;
            }
            // 握手阶段也可能收到 hook 事件（SessionStart hooks 在 initialize 前执行）
            if t == Some("system") {
                if let Some(sub) = v.get("subtype").and_then(|s| s.as_str()) {
                    if sub == "hook_started" || sub == "hook_response" {
                        let mut payload = v.clone();
                        payload
                            .as_object_mut()
                            .map(|o| o.insert("session_id".to_string(), json!(session_id)));
                        let _ = app.emit("session-hook", &payload);
                    }
                }
            }
            emit_fast_mode_status(app, session_id, &v);
        }
    }

    // 9. 默认开启 Remote Control（进程活着就有 RC，关进程自动断）
    let rc_msg = json!({
        "type": "control_request",
        "request_id": "rc-init",
        "request": {"subtype": "remote_control", "enabled": true}
    });
    let _ = write_stdin(&mut stdin, &rc_msg);

    // 10. 存入 ACTIVE_PROCESSES
    let pid = child.id();
    eprintln!(
        "[long-lived] 新建进程 PID={} 会话={}",
        pid,
        &session_id[..session_id.len().min(8)]
    );
    let started_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    // started_at_ms 随 connected 事件下发：warm 重启（needs_restart）时旧进程 EOF
    // 因 superseded 不发 exited，前端 processAlive 无边沿，此事件是代际锚点
    // 唯一不依赖竞态方向的刷新通道
    let _ = app.emit(
        "session-connected",
        json!({
            "session_id": session_id,
            "resumed": resumed,
            "cwd": cwd,
            "started_at_ms": started_at_ms,
        }),
    );
    let sp_arc = Arc::new(Mutex::new(SessionProcess {
        child,
        stdin,
        request_counter: 1,
        channel: channel.map(|s| s.to_string()),
        effort: effort.map(|s| s.to_string()),
        fast_mode,
        model: normalized_override(model).map(str::to_owned),
        permission_mode: normalized_override(permission_mode).map(str::to_owned),
        advisor,
        chrome,
        extra_args: extra_args.unwrap_or("").to_string(),
        capability_fingerprint: capability_bundle.fingerprint().to_string(),
        started_at_ms,
        pending_permission: HashMap::new(),
        init_reported: false,
        pending_injections: HashMap::new(),
        pending_controls: HashMap::new(),
        remote: remote.map(|launch| RemoteProcess {
            launch: launch.clone(),
            pid: remote_pid,
        }),
    }));
    if let (Some(launch), Some(pid)) = (remote, remote_pid) {
        remote_launch::record_pid(launch.instance.instance_id(), pid, &launch.cli_session_id);
    }
    ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(session_id.to_string(), sp_arc.clone());

    // 10a. 启动 stdout 读取线程（reader 已消费掉握手响应，后续全是流式事件）
    let handle = app.clone();
    let sid = session_id.to_string();
    let runtime_file = channel_injection.map(|inj| inj.runtime_path);
    std::thread::spawn(move || {
        read_stream(reader, stderr, sp_arc.clone(), &handle, &sid);
        PermissionService::stop_if_endpoint(&sid, &perm_endpoint);
        if let Some(p) = runtime_file {
            crate::channels::cleanup_runtime_file(&p);
        }
    });

    Ok(())
}

/// 发送消息（自动 open 会话进程；替代旧 start_streaming）
#[allow(clippy::too_many_arguments)] // 会话参数透传，拆结构体收益不大
pub fn send_message(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    message: &str,
    model: Option<&str>,
    effort: Option<&str>,
    fast_mode: Option<bool>,
    channel: Option<&str>,
    advisor: bool,
    chrome: bool,
    fork_source: Option<&str>,
    extra_args: Option<&str>,
    images: Option<&[serde_json::Value]>,
    permission_mode: Option<&str>,
    session_capabilities: Vec<SessionCapabilityId>,
    force_new: bool,
    remote: Option<&RemoteLaunch>,
) -> Result<(), String> {
    let capability_bundle = SessionCapabilityBundle::new(session_capabilities);
    let mut exists = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|m| m.contains_key(session_id));

    if exists {
        let needs_restart = ACTIVE_PROCESSES
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(session_id).cloned())
            .is_some_and(|arc| {
                let sp = arc.lock().unwrap();
                sp.channel.as_deref() != channel
                    || sp.effort.as_deref() != effort
                    // fastMode 经 --settings 注入，显式开关或恢复继承都需重启。
                    || sp.fast_mode != fast_mode
                    // advisor 经 --settings 注入,变更只能重启生效(否则只有主模型锁定生效、顾问没挂上)
                    || sp.advisor != advisor
                    // 模型意图回落默认(None)而进程钉着上次 set_model 的具体值:
                    // set_model 无法"切回默认",不重启就是旧模型粘滞(界面默认、实跑旧值)
                    || (model.is_none() && sp.model.is_some())
                    // set_permission_mode 无法撤销覆盖并重新读取磁盘配置。
                    || permission_change_requires_restart(
                        sp.permission_mode.as_deref(),
                        permission_mode,
                    )
                    // --chrome 是启动参数,开关变更(含关闭卸载浏览器工具省上下文)只能重启生效
                    || sp.chrome != chrome
                    // 自定义 CLI 参数同为启动参数,变更只能重启生效
                    || sp.extra_args != extra_args.unwrap_or("")
                    // Monet 专属能力由启动参数注入,集合变化只能重启生效
                    || capability_needs_restart(&sp.capability_fingerprint, &capability_bundle)
            });
        if needs_restart {
            eprintln!(
                "[long-lived] 启动配置变更，重启进程 会话={}",
                &session_id[..session_id.len().min(8)]
            );
            close_session(session_id);
            exists = false;
        }
    }

    if !exists {
        open_session(
            app,
            session_id,
            cwd,
            model,
            effort,
            fast_mode,
            channel,
            advisor,
            chrome,
            fork_source,
            extra_args,
            permission_mode,
            &capability_bundle,
            force_new,
            remote,
        )?;
    }

    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or("会话进程不存在")?;

    let mut sp = process.lock().unwrap();
    if exists {
        eprintln!(
            "[long-lived] 复用进程 PID={} 会话={}",
            sp.child.id(),
            &session_id[..session_id.len().min(8)]
        );
        if let Some(m) = model.filter(|s| !s.is_empty()) {
            sp.request_counter += 1;
            let req_id = format!("set-model-{}", sp.request_counter);
            let ctrl = json!({
                "type": "control_request",
                "request_id": req_id,
                "request": {"subtype": "set_model", "model": m}
            });
            write_stdin(&mut sp.stdin, &ctrl)?;
            sp.model = Some(m.to_string());
        }
    }

    if let Some(mode) = normalized_override(permission_mode) {
        request_permission_mode(app, &process, &mut sp, session_id, mode)?;
    }
    let mut content: Vec<serde_json::Value> = Vec::new();
    if let Some(imgs) = images {
        content.extend_from_slice(imgs);
    }
    content.push(json!({"type": "text", "text": message}));
    let msg = json!({
        "type": "user",
        "session_id": "",
        "message": {
            "role": "user",
            "content": content
        },
        "parent_tool_use_id": null
    });
    write_stdin(&mut sp.stdin, &msg)
}

/// 开关 Remote Control（通过 control_request 动态启用/禁用；进程未启动时自动连接）
#[allow(clippy::too_many_arguments)] // 会话参数透传，拆结构体收益不大
pub fn toggle_remote_control(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    model: Option<&str>,
    effort: Option<&str>,
    fast_mode: Option<bool>,
    channel: Option<&str>,
    advisor: bool,
    chrome: bool,
    fork_source: Option<&str>,
    extra_args: Option<&str>,
    enabled: bool,
    permission_mode: Option<&str>,
    session_capabilities: Vec<SessionCapabilityId>,
) -> Result<(), String> {
    let capability_bundle = SessionCapabilityBundle::new(session_capabilities);
    let mut exists = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|m| m.contains_key(session_id));

    if exists {
        let needs_restart = ACTIVE_PROCESSES
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(session_id).cloned())
            .is_some_and(|arc| {
                let sp = arc.lock().unwrap();
                // 远端进程不能按本机方式重启；开关直接走现有进程的控制请求
                sp.remote.is_none()
                    && (sp.fast_mode != fast_mode
                        || permission_change_requires_restart(
                            sp.permission_mode.as_deref(),
                            permission_mode,
                        )
                        || capability_needs_restart(&sp.capability_fingerprint, &capability_bundle))
            });
        if needs_restart {
            eprintln!(
                "[long-lived] Remote Control 启动配置变更，重启进程 会话={}",
                &session_id[..session_id.len().min(8)]
            );
            close_session(session_id);
            exists = false;
        }
    }

    if !exists {
        open_session(
            app,
            session_id,
            cwd,
            model,
            effort,
            fast_mode,
            channel,
            advisor,
            chrome,
            fork_source,
            extra_args,
            permission_mode,
            &capability_bundle,
            false,
            None,
        )?;
    }

    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or("会话进程不存在")?;

    let mut sp = process.lock().unwrap();
    if exists {
        if let Some(mode) = normalized_override(permission_mode) {
            request_permission_mode(app, &process, &mut sp, session_id, mode)?;
        }
    }
    eprintln!(
        "[long-lived] Remote Control enabled={} PID={} 会话={}",
        enabled,
        sp.child.id(),
        &session_id[..session_id.len().min(8)]
    );
    sp.request_counter += 1;
    // 语义编进 request_id：CLI 的 response 不含 enabled 值，read_stream 只能靠它还原请求意图
    let req_id = format!(
        "rc-{}-{}",
        if enabled { "on" } else { "off" },
        sp.request_counter
    );
    let msg = json!({
        "type": "control_request",
        "request_id": req_id,
        "request": {"subtype": "remote_control", "enabled": enabled}
    });
    write_stdin(&mut sp.stdin, &msg)
}

fn control_request(request_id: String, request: Value) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": request
    })
}

#[cfg(test)]
mod task_control_tests {
    use super::control_request;
    use serde_json::json;

    #[test]
    fn builds_background_and_interrupt_requests() {
        let background = control_request(
            "background-tasks-1".to_string(),
            json!({"subtype": "background_tasks"}),
        );
        let interrupt = control_request("interrupt-2".to_string(), json!({"subtype": "interrupt"}));
        assert_eq!(background["request"]["subtype"], "background_tasks");
        assert_eq!(interrupt["request"]["subtype"], "interrupt");
    }

    #[test]
    fn builds_single_task_stop_request() {
        let request = control_request(
            "stop-task-3".to_string(),
            json!({"subtype": "stop_task", "task_id": "task-17"}),
        );
        assert_eq!(request["request"]["subtype"], "stop_task");
        assert_eq!(request["request"]["task_id"], "task-17");
    }
}

/// 中断当前回复（不杀进程，也不终止其异步任务）。
///
/// CLI 的 interrupt 会清理仍处于前台的任务，因此必须先用 background_tasks 把它们
/// 从主轮次生命周期中分离；同一 stdin 上的两条控制消息按写入顺序处理。
pub fn interrupt_session(session_id: &str) -> Result<(), String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned());

    match process {
        Some(p) => {
            let mut sp = p.lock().unwrap();
            eprintln!(
                "[long-lived] 中断进程 PID={} 会话={}",
                sp.child.id(),
                &session_id[..session_id.len().min(8)]
            );
            sp.request_counter += 1;
            let background_id = format!("background-tasks-{}", sp.request_counter);
            let background = control_request(background_id, json!({"subtype": "background_tasks"}));
            write_stdin(&mut sp.stdin, &background)?;

            sp.request_counter += 1;
            let interrupt_id = format!("interrupt-{}", sp.request_counter);
            let interrupt = control_request(interrupt_id, json!({"subtype": "interrupt"}));
            write_stdin(&mut sp.stdin, &interrupt)
        }
        None => Ok(()),
    }
}

/// 按 CLI 任务 ID 终止一个异步任务，不影响主轮次及同会话其他任务。
pub fn stop_async_task(session_id: &str, task_id: &str) -> Result<(), String> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err("异步任务 ID 不能为空".to_string());
    }
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or_else(|| "会话进程不存在".to_string())?;

    let mut sp = process.lock().unwrap();
    sp.request_counter += 1;
    let request_id = format!("stop-task-{}", sp.request_counter);
    let request = control_request(
        request_id,
        json!({"subtype": "stop_task", "task_id": task_id}),
    );
    write_stdin(&mut sp.stdin, &request)
}

/// 运行时切换权限模式；判决经 `permission-mode-result` 信号回报
pub fn set_permission_mode(app: &AppHandle, session_id: &str, mode: &str) -> Result<(), String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned());

    match process {
        Some(p) => {
            let mut sp = p.lock().unwrap();
            request_permission_mode(app, &p, &mut sp, session_id, mode)
        }
        None => Err("会话进程不存在".to_string()),
    }
}

/// 发出权限模式切换。进程记录只在 CLI 判决成功后更新——CLI 对读不懂的请求会回
/// error，乐观更新会让界面显示新模式而进程仍按旧模式运行。
fn request_permission_mode(
    app: &AppHandle,
    process: &Arc<Mutex<SessionProcess>>,
    sp: &mut SessionProcess,
    session_id: &str,
    mode: &str,
) -> Result<(), String> {
    let already_pending = sp
        .pending_permission
        .values()
        .any(|pending| pending.requested == mode && !pending.timed_out);
    if sp.permission_mode.as_deref() == Some(mode) || already_pending {
        return Ok(());
    }
    sp.request_counter += 1;
    let req_id = format!(
        "{}{}",
        protocol::PERMISSION_REQUEST_PREFIX,
        sp.request_counter
    );
    write_stdin(
        &mut sp.stdin,
        &protocol::permission_mode_request(&req_id, mode),
    )?;
    sp.pending_permission.insert(
        req_id.clone(),
        PendingPermission {
            requested: mode.to_string(),
            timed_out: false,
        },
    );

    let app = app.clone();
    let process = Arc::downgrade(process);
    let sid = session_id.to_string();
    let requested = mode.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(PERMISSION_CONFIRM_TIMEOUT);
        let Some(process) = process.upgrade() else {
            return;
        };
        let confirmed = {
            let mut sp = process.lock().unwrap();
            match sp.pending_permission.get_mut(&req_id) {
                Some(pending) => pending.timed_out = true,
                None => return,
            }
            sp.permission_mode.clone()
        };
        emit_permission_result(
            &app,
            &sid,
            false,
            confirmed.as_deref(),
            &requested,
            Some("timeout"),
        );
    });
    Ok(())
}

fn emit_permission_result(
    app: &AppHandle,
    session_id: &str,
    ok: bool,
    mode: Option<&str>,
    requested: &str,
    error: Option<&str>,
) {
    dispatch_stream_signal(
        app,
        "permission-mode-result",
        json!({
            "session_id": session_id,
            "ok": ok,
            "mode": mode,
            "requested": requested,
            "error": error,
        }),
    );
}

/// CLI 对权限切换的判决：成功才落进程记录；失败回报进程当前真实模式供界面回退
fn settle_permission_response(
    app: &AppHandle,
    process: &Arc<Mutex<SessionProcess>>,
    session_id: &str,
    request_id: &str,
    outcome: &ControlOutcome,
) {
    let (requested, confirmed) = {
        let mut sp = process.lock().unwrap();
        let Some(pending) = sp.pending_permission.remove(request_id) else {
            return;
        };
        if *outcome == ControlOutcome::Success {
            sp.permission_mode = Some(pending.requested.clone());
        }
        (pending.requested, sp.permission_mode.clone())
    };
    match outcome {
        ControlOutcome::Success => {
            emit_permission_result(app, session_id, true, Some(&requested), &requested, None)
        }
        ControlOutcome::Error(error) => emit_permission_result(
            app,
            session_id,
            false,
            confirmed.as_deref(),
            &requested,
            Some(error),
        ),
    }
}

/// 把一条用户消息插入正在运行的轮次，返回关联 uuid。
///
/// CLI 在下一个工具间隙并入当前轮次；轮次先结束则另起一轮处理。命运经
/// `session-injection` 事件回报（queued / started / 终态 / timeout）。
/// 5 秒内没有任何回报视为未送达：撤回该消息，避免迟到执行与用户重发重复。
pub fn inject_message(
    app: &AppHandle,
    session_id: &str,
    message: &str,
    images: Option<&[serde_json::Value]>,
) -> Result<String, String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or("会话进程不存在")?;

    let uuid = uuid::Uuid::new_v4().to_string();
    let mut content: Vec<serde_json::Value> = images.map(<[_]>::to_vec).unwrap_or_default();
    content.push(json!({"type": "text", "text": message}));
    {
        let mut sp = process.lock().unwrap();
        write_stdin(
            &mut sp.stdin,
            &protocol::mid_turn_user_message(&uuid, content),
        )?;
        sp.pending_injections.insert(
            uuid.clone(),
            PendingInjection {
                acknowledged: false,
            },
        );
    }

    let app = app.clone();
    let process = Arc::downgrade(&process);
    let sid = session_id.to_string();
    let message_uuid = uuid.clone();
    std::thread::spawn(move || {
        std::thread::sleep(INJECTION_ACK_TIMEOUT);
        let Some(process) = process.upgrade() else {
            return;
        };
        {
            let mut sp = process.lock().unwrap();
            let awaiting = sp
                .pending_injections
                .get(&message_uuid)
                .is_some_and(|pending| !pending.acknowledged);
            if !awaiting {
                return;
            }
            sp.request_counter += 1;
            let req_id = format!(
                "{}{}",
                protocol::CANCEL_INJECTION_PREFIX,
                sp.request_counter
            );
            if let Err(error) = write_stdin(
                &mut sp.stdin,
                &protocol::cancel_async_message_request(&req_id, &message_uuid),
            ) {
                eprintln!("[long-lived] 撤回插入消息失败: {error}");
            }
        }
        emit_injection_state(&app, &sid, &message_uuid, "timeout");
    });
    Ok(uuid)
}

/// 回退某条用户消息之后的文件改动（dry_run 只预览）。改动由 CLI 按文件快照执行，
/// Monet 不直接写任何文件；只作用于本窗口正在运行的会话进程。
pub fn rewind_files(
    session_id: &str,
    user_message_id: &str,
    dry_run: bool,
) -> Result<protocol::RewindFilesResult, String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or("会话进程不存在")?;
    let (reply, receiver) = std::sync::mpsc::channel();
    let request_id = {
        let mut sp = process.lock().unwrap();
        sp.request_counter += 1;
        let request_id = format!(
            "{}{}",
            protocol::REWIND_FILES_REQUEST_PREFIX,
            sp.request_counter
        );
        write_stdin(
            &mut sp.stdin,
            &protocol::rewind_files_request(&request_id, user_message_id, dry_run),
        )?;
        sp.pending_controls.insert(request_id.clone(), reply);
        request_id
    };
    let outcome = receiver.recv_timeout(REWIND_FILES_TIMEOUT);
    process.lock().unwrap().pending_controls.remove(&request_id);
    match outcome {
        Ok(Ok(body)) => Ok(protocol::parse_rewind_files_result(&body)),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("timeout".to_string()),
    }
}

fn emit_injection_state(app: &AppHandle, session_id: &str, uuid: &str, state: &str) {
    dispatch_stream_signal(
        app,
        "session-injection",
        json!({ "session_id": session_id, "uuid": uuid, "state": state }),
    );
}

/// 关闭会话进程（SIGTERM → 5s → SIGKILL）
pub fn close_session(session_id: &str) {
    let process_arc = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|m| m.remove(session_id));

    if let Some(process_arc) = process_arc {
        let (pid, remote) = {
            let mut sp = process_arc.lock().unwrap();
            graceful_shutdown(&mut sp);
            (sp.child.id(), sp.remote.clone())
        };
        // 远端会话：结束本机 ssh 即关闭远端 stdin；宽限后远端 pid 仍是这条会话就精确 kill
        if let Some(RemoteProcess {
            launch,
            pid: Some(remote_pid),
        }) = remote
        {
            std::thread::spawn(move || {
                std::thread::sleep(REMOTE_EXIT_GRACE);
                remote_launch::kill_if_ours(&launch, remote_pid);
            });
        }
        signal_session(pid, "-TERM");

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if let Ok(mut sp) = process_arc.lock() {
                if sp.child.try_wait().ok().flatten().is_none() {
                    signal_session(pid, "-KILL");
                    let _ = sp.child.wait();
                }
            }
        });
    }

    PermissionService::stop_for(session_id);
}

/// 关闭所有活跃会话进程（应用退出时调用）。
/// 必须同步等待：主进程退出后无人兜底，close_session 那种 detached 线程在此场景随进程消亡。
/// SIGTERM 全体 → 轮询最多 400ms → 对未退者 SIGKILL 整组。窗口收窄的依据：进程组 SIGKILL
/// 本身就保证不留孤儿，JSONL 逐行落盘强杀不丢会话数据——优雅窗口只是让 CLI 少打错误日志，
/// 不值得让退出卡秒级（quit_app 同步调用本函数，等待时长 = 用户按 Cmd+Q 的延迟）。
pub fn close_all_sessions() {
    let processes: Vec<(String, Arc<Mutex<SessionProcess>>)> = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_mut()
        .map(|m| m.drain().collect())
        .unwrap_or_default();

    let mut pids: Vec<u32> = Vec::with_capacity(processes.len());
    for (sid, process_arc) in &processes {
        if let Ok(mut sp) = process_arc.lock() {
            graceful_shutdown(&mut sp);
            let pid = sp.child.id();
            signal_session(pid, "-TERM");
            pids.push(pid);
        }
        PermissionService::stop_for(sid);
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(400);
    loop {
        let all_exited = processes.iter().all(|(_, arc)| {
            arc.lock()
                .map(|mut sp| sp.child.try_wait().ok().flatten().is_some())
                .unwrap_or(true)
        });
        if all_exited || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    for (i, (_, arc)) in processes.iter().enumerate() {
        if let Ok(mut sp) = arc.lock() {
            if sp.child.try_wait().ok().flatten().is_none() {
                signal_session(pids[i], "-KILL");
                let _ = sp.child.wait();
            }
        }
    }
}

/// 定向发信号：group=true 发整组（pgid = pid），否则单 PID。清扫场景不做回退——
/// 老残留进程的 pgid 可能是终端 shell 组，组信号会误伤，必须按 pgid==pid 判定后精确选择。
#[cfg(unix)]
fn send_sig(pid: u32, sig: &str, group: bool) {
    if group {
        let _ = Command::new("kill")
            .args([sig, "--", &format!("-{}", pid)])
            .output();
    } else {
        let _ = Command::new("kill").args([sig, &pid.to_string()]).output();
    }
}

/// 启动残留清扫：上次崩溃/强杀/dev Ctrl+C 遗留的会话进程树。
/// 指纹：命令行含 `.monet/runtime/`（Monet 注入的 --settings 路径）或 `/monet-mcp`。
/// ppid==1 是必要条件（父进程已死被 launchd 收养）——其他 Monet 实例的活会话 ppid 是该实例，不会命中。
/// 须在本实例 spawn 任何会话之前调用（setup 阶段）。
pub fn cleanup_orphans() {
    #[cfg(unix)]
    {
        let output = match Command::new("ps")
            .args(["-axo", "pid=,ppid=,pgid=,command="])
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return,
        };
        let text = String::from_utf8_lossy(&output.stdout);
        // (pid, 可整组杀)。--append-system-prompt 含真实换行会把命令行拆成多行，
        // 但指纹字段（--settings/--mcp-config）位于参数前段与 pid 同行，续行解析 pid 失败自然跳过
        let mut targets: Vec<(u32, bool)> = Vec::new();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(pid), Some(ppid), Some(pgid)) = (
                parts.next().and_then(|s| s.parse::<u32>().ok()),
                parts.next().and_then(|s| s.parse::<u32>().ok()),
                parts.next().and_then(|s| s.parse::<u32>().ok()),
            ) else {
                continue;
            };
            if ppid != 1 {
                continue;
            }
            if !(line.contains(".monet/runtime/") || line.contains("/monet-mcp")) {
                continue;
            }
            targets.push((pid, pgid == pid));
        }
        if targets.is_empty() {
            return;
        }
        eprintln!(
            "[cleanup_orphans] 清理 {} 个残留会话进程: {:?}",
            targets.len(),
            targets.iter().map(|t| t.0).collect::<Vec<_>>()
        );
        for (pid, grouped) in &targets {
            send_sig(*pid, "-TERM", *grouped);
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            for (pid, grouped) in &targets {
                let alive = Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if alive {
                    send_sig(*pid, "-KILL", *grouped);
                }
            }
        });
    }
}

fn graceful_shutdown(sp: &mut SessionProcess) {
    sp.request_counter += 1;
    let req_id = format!("shutdown-{}", sp.request_counter);
    let _ = write_stdin(
        &mut sp.stdin,
        &json!({
            "type": "control_request",
            "request_id": req_id,
            "request": {"subtype": "remote_control", "enabled": false}
        }),
    );
}

fn emit_error(app: &AppHandle, session_id: &str, message: String) {
    dispatch_stream_event(
        app,
        StreamEvent::Error {
            session_id: session_id.to_string(),
            message,
        },
    );
    dispatch_stream_signal(app, "stream-done", json!({ "session_id": session_id }));
}

/// 定位 monet-mcp 二进制
///
/// 顺序：
/// 1. 环境变量 `MONET_MCP_BIN`（手动覆盖）
/// 2. 与当前进程同目录（生产打包后 sidecar 就在主程序旁）
/// 3. cargo target 目录（开发模式：current_exe 直接就是 target/{debug,release}/app）
fn find_mcp_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MONET_MCP_BIN") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let (bundled, dev) = if cfg!(windows) {
        ("monet-mcp.exe", "monet_mcp.exe")
    } else {
        ("monet-mcp", "monet_mcp")
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(bundled);
            if candidate.is_file() {
                return Some(candidate);
            }
            // dev: tauri 启动时 current_exe 在 target/debug，二进制名为 monet_mcp
            let candidate2 = dir.join(dev);
            if candidate2.is_file() {
                return Some(candidate2);
            }
        }
    }
    None
}

/// 读取长活进程 stdout——逐行解析流式事件，result 立即 emit + stream-done 标记轮次结束，
/// 循环仅在 stdout EOF（进程退出）时终止
fn fast_mode_status_notification(value: &Value) -> Option<bool> {
    if value.get("type").and_then(Value::as_str) != Some("system")
        || value.get("subtype").and_then(Value::as_str) != Some("notification")
    {
        return None;
    }
    // CLI 目前把原因放在 notification 的文本字段中；序列化整条事件兼容字段名演进，
    // 但仍限定 system/notification + Fast mode，避免误认普通模型输出。
    let text = value.to_string().to_ascii_lowercase();
    if !text.contains("fast mode") && !text.contains("fast-mode") {
        return None;
    }
    if text.contains("re-enabling fast mode")
        || text.contains("fast-mode-cooldown-expired")
        || text.contains("fast mode enabled")
        || text.contains("fast mode active")
    {
        return Some(true);
    }
    if text.contains("disabled")
        || text.contains("unavailable")
        || text.contains("not available")
        || text.contains("not enabled")
        || text.contains("requires")
        || text.contains("rejected")
        || text.contains("fast-mode-cooldown-started")
        || text.contains("fast-mode-overage-rejected")
        || text.contains("exhausted")
        || text.contains("limit reached")
    {
        return Some(false);
    }
    None
}

fn emit_fast_mode_status(app: &AppHandle, session_id: &str, value: &Value) {
    if let Some(active) = fast_mode_status_notification(value) {
        dispatch_stream_signal(
            app,
            "fast-mode-status",
            json!({ "session_id": session_id, "active": active }),
        );
    }
}

#[cfg(test)]
mod fast_mode_status_tests {
    use super::fast_mode_status_notification;
    use serde_json::json;

    #[test]
    fn decodes_fast_mode_fallback_notifications() {
        assert_eq!(
            fast_mode_status_notification(&json!({
                "type": "system",
                "subtype": "notification",
                "notification_type": "fast-mode-overage-rejected",
                "message": "Fast mode disabled — usage credits exhausted"
            })),
            Some(false)
        );
    }

    #[test]
    fn decodes_fast_mode_reenable_notifications() {
        assert_eq!(
            fast_mode_status_notification(&json!({
                "type": "system",
                "subtype": "notification",
                "message": "Fast mode cooldown expired, re-enabling fast mode"
            })),
            Some(true)
        );
    }

    #[test]
    fn ignores_unrelated_notifications() {
        assert_eq!(
            fast_mode_status_notification(&json!({
                "type": "system",
                "subtype": "notification",
                "message": "Background task completed"
            })),
            None
        );
    }
}

fn read_stream(
    reader: BufReader<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    process: Arc<Mutex<SessionProcess>>,
    app: &AppHandle,
    session_id: &str,
) {
    // (message_id, model):message_start 写入,后续 content_block_* 读取;model 是本轮真值
    let mut current_message_id: Option<(String, Option<String>)> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let raw_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

        if let Some(event) = decode_stream_event(&value, session_id, &mut current_message_id) {
            dispatch_stream_event(app, event);
        }

        // hook 事件转发给前端（system 类型的 hook_started / hook_response）
        if raw_type == "system" {
            if let Some(subtype) = value.get("subtype").and_then(|s| s.as_str()) {
                if subtype == "hook_started" || subtype == "hook_response" {
                    let mut payload = value.clone();
                    payload
                        .as_object_mut()
                        .map(|o| o.insert("session_id".to_string(), json!(session_id)));
                    let _ = app.emit("session-hook", &payload);
                }
            }
        }
        emit_fast_mode_status(app, session_id, &value);
        route_protocol_signals(app, &process, session_id, &value);

        // Remote Control 判决回报：success/error 都上报，前端 rcActive 完全由本事件驱动
        // （不能乐观置位——判决与 invoke 返回并发，先到的判决会被晚写的乐观值覆盖）。
        // 请求意图编码在 request_id：rc-init（连接自动开）/rc-on-N/rc-off-N（手动）。
        // 第三方渠道实测 CLI 回 error "Remote Control initialization failed"
        if raw_type == "control_response" {
            if let Some(resp) = value.get("response") {
                let req_id = resp
                    .get("request_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if req_id.starts_with("rc-") {
                    let is_err = resp.get("subtype").and_then(|v| v.as_str()) == Some("error");
                    let wanted_on = !req_id.starts_with("rc-off");
                    // 成功 = 达成所愿；失败 = 维持原状（开失败→仍关，关失败→仍开）
                    let active = if is_err { !wanted_on } else { wanted_on };
                    let err = resp.get("error").and_then(|v| v.as_str());
                    eprintln!(
                        "[long-lived] Remote Control 判决 req={} active={} 会话={}{}",
                        req_id,
                        active,
                        &session_id[..session_id.len().min(8)],
                        err.map(|e| format!(" err={}", e)).unwrap_or_default()
                    );
                    let _ = app.emit("rc-status", json!({
                        "session_id": session_id,
                        "active": active,
                        // 手动开关的判决才触发 toast；连接时自动开启的失败静默（防第三方渠道每次连接被骚扰）
                        "manual": req_id != "rc-init",
                        "error": if is_err { json!(err.unwrap_or("Remote Control unavailable")) } else { Value::Null },
                    }));
                }
            }
        }

        // "result" 标记一轮结束（进程继续活着等下一条 stdin 消息）。
        // 轮次归属：CLI 自发轮（task-notification 唤醒的后台任务收尾轮）的 result 带
        // origin 字段（如 {"kind":"task-notification"}），用户 send 的轮没有——实测确认。
        // 前端靠 initiator 区分：auto 轮不得冒领用户消息的 streaming 标志/落账收尾，
        // 否则后台任务期间发消息会出现轮次边界错乱（消息重复出现/消失）
        if raw_type == "result" {
            let is_auto = value.get("origin").is_some_and(|v| !v.is_null());
            dispatch_stream_signal(
                app,
                "stream-done",
                json!({
                    "session_id": session_id,
                    "initiator": if is_auto { "auto" } else { "user" },
                }),
            );
        }
    }

    // stdout EOF —— 进程已退出

    // 所有退出路径都在 reader 线程统一 wait。close_session / 热重启会先从活跃表移除，
    // 但 `Child` Drop 本身不会回收退出状态；只在“非预期退出”分支 wait 会让预期关闭
    // 与被新代际顶替的进程短暂或永久成为 zombie。
    let (exit_status, remote) = {
        let mut sp = process.lock().unwrap();
        (sp.child.wait().ok(), sp.remote.clone())
    };
    // ssh 以 255 退出 = 连接中断，远端进程可能还活着：保留 pid 记录，下一轮同步或重开该会话时清理
    let disconnected = remote.is_some()
        && exit_status.is_some_and(|status| status.code() == Some(SSH_CONNECTION_LOST));
    if let Some(RemoteProcess {
        launch,
        pid: Some(pid),
    }) = &remote
    {
        if disconnected {
            remote_launch::mark_exited(launch.instance.instance_id(), *pid);
        } else {
            remote_launch::forget_pid(launch.instance.instance_id(), *pid);
        }
    }

    // 判断退出性质（close_session 会先从 map 移除再 SIGTERM）：
    // - map 中仍是本进程 = 非预期退出，需要上报错误
    // - map 中是别的 Arc = 已被新进程顶替（模型/渠道变更重启），会话进程实际还活着
    let (was_unexpected, superseded) = {
        let mut map = ACTIVE_PROCESSES.lock().unwrap();
        let is_self = map
            .as_ref()
            .and_then(|m| m.get(session_id))
            .is_some_and(|c| Arc::ptr_eq(c, &process));
        if is_self {
            if let Some(m) = map.as_mut() {
                m.remove(session_id);
            }
            (true, false)
        } else {
            let superseded = map.as_ref().is_some_and(|m| m.contains_key(session_id));
            (false, superseded)
        }
    };

    // 进程退出的独立信号（与 stream-done 的"轮次结束"语义分离）：
    // 前端靠它维护 processAlive——turn 结束后长活进程仍在跑后台任务（Workflow/子 agent）
    // 时，异步账本据此把无终态条目判为 running 而非 unknown。被顶替时不发，防止
    // 旧进程 EOF 迟到把新进程的活态翻掉。
    if !superseded {
        dispatch_stream_signal(
            app,
            "session-process-exited",
            json!({ "session_id": session_id }),
        );
    }

    if was_unexpected && disconnected {
        if let Some(RemoteProcess { launch, .. }) = &remote {
            emit_error(
                app,
                session_id,
                format!("REMOTE_DISCONNECTED:{}", launch.alias),
            );
        }
    } else if was_unexpected {
        if let Some(status) = exit_status {
            if !status.success() {
                let stderr_text = stderr.map(|stderr| {
                    let mut reader = BufReader::new(stderr);
                    let mut buf = String::new();
                    let _ = std::io::Read::read_to_string(&mut reader, &mut buf);
                    buf
                });
                let err_msg = stderr_text
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| t.trim().to_string())
                    .unwrap_or_else(|| format!("进程异常退出 (code: {})", status));
                emit_error(app, session_id, err_msg);
            } else {
                // 正常退出但未预期（CLI 自行决定退出），通知前端收尾
                dispatch_stream_signal(app, "stream-done", json!({ "session_id": session_id }));
            }
        } else {
            emit_error(app, session_id, "进程异常退出".to_string());
        }
    }
}

/// Claude 协议的扩展信号：控制判决、反向请求兜底、限额、init 问题、实时状态与未知采样。
/// 只做翻译与转发；任何一项解析失败都只是跳过，不影响主流读取。
fn route_protocol_signals(
    app: &AppHandle,
    process: &Arc<Mutex<SessionProcess>>,
    session_id: &str,
    value: &Value,
) {
    if let Some((request_id, outcome, body)) = protocol::control_response(value) {
        if request_id.starts_with(protocol::PERMISSION_REQUEST_PREFIX) {
            settle_permission_response(app, process, session_id, &request_id, &outcome);
        } else if let Some(reply) = process.lock().unwrap().pending_controls.remove(&request_id) {
            let _ = reply.send(match outcome {
                ControlOutcome::Success => Ok(body.cloned().unwrap_or(Value::Null)),
                ControlOutcome::Error(error) => Err(error),
            });
        }
        return;
    }

    if let Some(lifecycle) = protocol::parse_command_lifecycle(value) {
        // 只转发 Monet 自己插入的消息；超时后迟到的回报照常转发，前端据此认领
        let tracked = {
            let mut sp = process.lock().unwrap();
            if lifecycle.state.is_terminal() {
                sp.pending_injections
                    .remove(&lifecycle.command_uuid)
                    .is_some()
            } else if let Some(pending) = sp.pending_injections.get_mut(&lifecycle.command_uuid) {
                pending.acknowledged = true;
                true
            } else {
                false
            }
        };
        if tracked {
            emit_injection_state(
                app,
                session_id,
                &lifecycle.command_uuid,
                lifecycle.state.as_str(),
            );
        }
        return;
    }

    // 远端会话以 stdio 权限通道启动：审批走与本机相同的前端流程，决定写回同一条管道
    if let Some(request) = protocol::parse_can_use_tool(value) {
        if process.lock().unwrap().remote.is_some() {
            let target = process.clone();
            let request_id = request.request_id.clone();
            let accepted = PermissionService::request_via_stdio(
                app,
                session_id,
                request.tool_name,
                request.input,
                move |decision| {
                    let reply = protocol::tool_permission_response(&request_id, decision);
                    if let Ok(mut sp) = target.lock() {
                        if let Err(error) = write_stdin(&mut sp.stdin, &reply) {
                            eprintln!("[long-lived] 远端权限应答写入失败: {error}");
                        }
                    }
                },
            );
            if accepted {
                return;
            }
        }
    }

    match protocol::classify_reverse_request(value) {
        ReverseRequest::Reject { subtype, response } => {
            // 立即应答，CLI 走取消分支而不是挂到超时
            if let Err(error) = write_stdin(&mut process.lock().unwrap().stdin, &response) {
                eprintln!("[long-lived] 反向请求兜底应答写入失败 subtype={subtype}: {error}");
            }
            if subtype == "can_use_tool" {
                eprintln!("[long-lived] warn: 收到 can_use_tool，已按不支持拒绝");
            }
            diagnostics::record_unknown(value);
            dispatch_stream_signal(
                app,
                "session-notice",
                json!({ "session_id": session_id, "kind": "unsupported_request", "subtype": subtype }),
            );
            return;
        }
        ReverseRequest::MissingId { subtype } => {
            eprintln!("[long-lived] 反向请求缺少 request_id，未应答 subtype={subtype}");
            return;
        }
        ReverseRequest::NotARequest => {}
    }

    if let Some(signal) = protocol::parse_rate_limit_event(value) {
        // 额度面板据此暂停 usage API 自动轮询（跨进程经快照文件共享给托盘）
        crate::quota::record_live_rate_limit(&crate::quota::LiveRateLimitSnapshot {
            rate_limit_type: signal.rate_limit_type.clone(),
            utilization: signal.utilization,
            resets_at_ms: signal.resets_at_ms,
            received_at_ms: chrono::Utc::now().timestamp_millis(),
        });
        let mut payload = serde_json::to_value(&signal).unwrap_or_else(|_| json!({}));
        if let Some(object) = payload.as_object_mut() {
            object.insert("session_id".to_string(), json!(session_id));
        }
        dispatch_stream_signal(app, "session-rate-limit", payload);
        return;
    }

    if let Some(report) = protocol::parse_system_init(value) {
        if let Some(version) = report.cli_version.as_deref() {
            diagnostics::note_cli_version(version);
        }
        let first = {
            let mut sp = process.lock().unwrap();
            !std::mem::replace(&mut sp.init_reported, true)
        };
        if first && !report.issues.is_empty() {
            dispatch_stream_signal(
                app,
                "session-notice",
                json!({ "session_id": session_id, "kind": "init_issues", "items": report.issues }),
            );
        }
        return;
    }

    if let Some(signal) = protocol::parse_live_signal(value) {
        let name = match signal {
            LiveSignal::ModelFallback { .. } | LiveSignal::PermissionDenied { .. } => {
                "session-notice"
            }
            _ => "session-live-status",
        };
        let mut payload = serde_json::to_value(&signal).unwrap_or_else(|_| json!({}));
        if let Some(object) = payload.as_object_mut() {
            object.insert("session_id".to_string(), json!(session_id));
        }
        dispatch_stream_signal(app, name, payload);
        return;
    }

    if !protocol::is_known_event(value) {
        diagnostics::record_unknown(value);
    }
}

/// 解析单行 stream-json 为 StreamEvent
///
/// `current_message_id` 由 `stream_event/message_start` 写入，后续 content_block_* 事件读取——
/// CLI 的 partial messages envelope 是 Anthropic Messages API 原生 SSE 的透传，
/// 单个 block 事件本身不带 message_id，需要靠跨行状态拼接。
fn decode_stream_event(
    value: &Value,
    session_id: &str,
    current_message_id: &mut Option<(String, Option<String>)>,
) -> Option<StreamEvent> {
    let event_type = value.get("type")?.as_str()?;
    let sid = session_id.to_string();

    // 子 agent 的转发事件：顶层 parent_tool_use_id 非 null（实测 v2.1.x 子 agent 的
    // assistant 快照带此标记，主对话恒 null）。不进主对话流——子 agent 内容由
    // 异步面板从 subagents 转录呈现，混入主流会把子 agent 输出渲染进主会话区
    if value
        .get("parent_tool_use_id")
        .is_some_and(|v| !v.is_null())
    {
        return None;
    }

    match event_type {
        "stream_event" => {
            // CLI envelope: { type: "stream_event", event: <Anthropic SSE event> }
            let inner = value.get("event")?;
            let inner_type = inner.get("type")?.as_str()?;
            match inner_type {
                "message_start" => {
                    // 仅更新跨行状态,不 emit:让 content_block_start 自带建 turn 能力。
                    // 顺带提取 message.model——本轮实际运行模型的最早真值来源
                    let msg = inner.get("message")?;
                    let id = msg.get("id")?.as_str()?.to_string();
                    let model = msg.get("model").and_then(|v| v.as_str()).map(String::from);
                    *current_message_id = Some((id, model));
                    None
                }
                "content_block_start" => {
                    let (mid, model) = current_message_id.as_ref()?.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    let cb_value = inner.get("content_block")?.clone();
                    let content_block: ContentBlock = serde_json::from_value(cb_value).ok()?;
                    Some(StreamEvent::BlockStart {
                        session_id: sid,
                        message_id: mid,
                        index,
                        content_block,
                        model,
                    })
                }
                "content_block_delta" => {
                    let mid = current_message_id.as_ref()?.0.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    let delta = inner.get("delta")?.clone();
                    Some(StreamEvent::BlockDelta {
                        session_id: sid,
                        message_id: mid,
                        index,
                        delta,
                    })
                }
                "content_block_stop" => {
                    let mid = current_message_id.as_ref()?.0.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    Some(StreamEvent::BlockStop {
                        session_id: sid,
                        message_id: mid,
                        index,
                    })
                }
                // message_delta / message_stop 忽略,终结由 result 事件兜底
                _ => None,
            }
        }
        "assistant" => {
            let msg = value.get("message")?;
            let message_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("uuid").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_string();
            let content: Vec<ContentBlock> = msg
                .get("content")
                .and_then(|c| serde_json::from_value(c.clone()).ok())
                .unwrap_or_default();
            let model = msg.get("model").and_then(|v| v.as_str()).map(String::from);
            let usage = msg.get("usage").cloned();
            let stop_reason = msg
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(String::from);
            Some(StreamEvent::AssistantMessage {
                session_id: sid,
                message_id,
                content,
                model,
                usage,
                stop_reason,
            })
        }
        "user" => {
            // 工具执行完成后,CLI 会发一条 user message 把 tool_result 回灌给
            // agent loop。它不是 Anthropic 的 partial message,但在最终 result
            // 之前已经可用,应及时送到前端显示。
            let content = value
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| {
                    serde_json::from_value::<Vec<ContentBlock>>(content.clone()).ok()
                })?;
            let tool_results: Vec<ContentBlock> = content
                .into_iter()
                .filter(|block| matches!(block, ContentBlock::ToolResult { .. }))
                .collect();
            if tool_results.is_empty() {
                return None;
            }
            Some(StreamEvent::ToolResults {
                session_id: sid,
                message_id: current_message_id.as_ref().map(|(id, _)| id.clone()),
                content: tool_results,
            })
        }
        // "progress"（老版 CLI 的子任务进度转发容器）不再混入主流：
        // 子 agent 内容归异步面板，主对话只渲染自己的消息
        "result" => {
            let result_text = value
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_error = value
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let is_interrupt =
                result_text.is_empty() || result_text.contains("interrupted by user");
            if is_error && !is_interrupt {
                Some(StreamEvent::Error {
                    session_id: sid,
                    message: result_text,
                })
            } else {
                let cost = value
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let usage = value.get("modelUsage");
                Some(StreamEvent::Result {
                    session_id: sid,
                    text: result_text,
                    cost_usd: cost,
                    context_window: usage
                        .and_then(|u| u.get("contextWindow"))
                        .and_then(|v| v.as_u64()),
                    input_tokens: usage
                        .and_then(|u| u.get("inputTokens"))
                        .and_then(|v| v.as_u64()),
                    output_tokens: usage
                        .and_then(|u| u.get("outputTokens"))
                        .and_then(|v| v.as_u64()),
                })
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod decode_stream_event_tests {
    use super::*;

    #[test]
    fn decodes_tool_results_from_user_message() {
        let value = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_bash",
                        "content": "hello\\nworld",
                        "is_error": false
                    },
                    {
                        "type": "text",
                        "text": "ignored"
                    }
                ]
            }
        });
        let mut current_message_id = Some(("message-1".to_string(), None));

        let event = decode_stream_event(&value, "session-1", &mut current_message_id)
            .expect("tool result should produce a stream event");
        match event {
            StreamEvent::ToolResults {
                session_id,
                message_id,
                content,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(message_id.as_deref(), Some("message-1"));
                assert_eq!(content.len(), 1);
                assert!(matches!(
                    content.first(),
                    Some(ContentBlock::ToolResult { tool_use_id, .. }) if tool_use_id == "toolu_bash"
                ));
            }
            other => panic!("expected tool results, got {other:?}"),
        }
    }

    #[test]
    fn ignores_non_tool_user_message() {
        let value = json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": "hello"}]}
        });
        let mut current_message_id = None;

        assert!(decode_stream_event(&value, "session-1", &mut current_message_id).is_none());
    }

    #[test]
    fn keeps_tool_block_start_when_input_arrives_later() {
        let value = json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_read",
                    "name": "Read"
                }
            }
        });
        let mut current_message_id = Some(("message-1".to_string(), None));

        let event = decode_stream_event(&value, "session-1", &mut current_message_id)
            .expect("tool block start must be visible before input deltas arrive");
        match event {
            StreamEvent::BlockStart { content_block, .. } => {
                assert!(matches!(
                    content_block,
                    ContentBlock::ToolUse { input, .. }
                        if input == json!({})
                ));
            }
            other => panic!("expected block start, got {other:?}"),
        }
    }
}
