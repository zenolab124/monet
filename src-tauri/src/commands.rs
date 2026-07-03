use std::fs;
use std::path::PathBuf;

use crate::discovery;
use crate::models::*;
use crate::parser;
use crate::permission::PermissionService;
use crate::probe;
use crate::streaming;
use crate::usage_stats;

/// 获取所有项目（含会话摘要）。async：全量扫描属重活，避免阻塞主线程排队其他 IPC
#[tauri::command]
pub async fn get_projects() -> Vec<Project> {
    discovery::discover_all()
}

/// 性能监视 HUD 数据源：app 相关进程的真实内存足迹
#[tauri::command]
pub async fn get_perf_stats() -> crate::perf::PerfStats {
    crate::perf::collect()
}

/// 获取会话消息记录（仅 user/assistant，跳过 snapshot 等大型记录）。async 同上
#[tauri::command]
pub async fn get_session_records(project_id: String, session_id: String) -> Vec<SessionRecord> {
    let path = session_path(&project_id, &session_id);
    let t0 = std::time::Instant::now();
    let records = parser::parse_messages(&path);
    if cfg!(debug_assertions) {
        // dev 埋点:与前端 [perf] 会话加载报告的 invoke 段对照,差值即 IPC 序列化成本
        eprintln!(
            "[perf] parse_messages {}: {} records · {:.1}ms",
            &session_id[..session_id.len().min(8)],
            records.len(),
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    records
}

/// 获取单个会话的摘要信息。走摘要缓存：projects-changed 增量路径高频调用，
/// 顺带热缓存使下次全量扫描免于重复解析
#[tauri::command]
pub async fn get_session_summary(
    project_id: String,
    session_id: String,
) -> Option<SessionSummary> {
    let path = session_path(&project_id, &session_id);
    crate::cache::get_summary(&path)
}

/// 删除会话（.jsonl 文件 + 子会话目录）
#[tauri::command]
pub fn delete_session(project_id: String, session_id: String) -> Result<(), String> {
    let jsonl = session_path(&project_id, &session_id);
    if jsonl.exists() {
        fs::remove_file(&jsonl).map_err(|e| e.to_string())?;
    }
    // 删除可能存在的子会话目录
    let subdir = projects_dir().join(&project_id).join(&session_id);
    if subdir.is_dir() {
        fs::remove_dir_all(&subdir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 在终端中恢复会话。channel 非空时经 `--settings <渠道文件>` 带上会话渠道——
/// 终端用渠道原文件(非 runtime 合成):与「终端可直接复用渠道文件」的设计一致,
/// 终端环境的变量残留属用户自己的 shell 管辖,不做防御注入
#[tauri::command]
pub async fn resume_in_terminal(
    cwd: String,
    session_id: String,
    channel: Option<String>,
) -> Result<(), String> {
    let settings_part = match channel
        .as_deref()
        .filter(|c| !c.is_empty() && *c != crate::channels::OFFICIAL_ID)
    {
        Some(ch) => {
            crate::channels::validate_id(ch)?;
            let path = crate::channels::channel_file_path(ch);
            if !path.is_file() {
                return Err(format!("渠道配置不存在: {}", ch));
            }
            format!(" --settings \\\"{}\\\"", path.display())
        }
        None => String::new(),
    };
    let escaped_cwd = cwd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "cd \"{}\" && claude{} --resume {}"
        end tell"#,
        escaped_cwd, settings_part, session_id
    );
    // osascript 会阻塞在系统自动化授权弹窗上：放 blocking 线程等待结果，
    // 授权被拒（-1743）时返回前端可识别的错误标记
    let output = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("-1743") || stderr.contains("Not authorized") {
            return Err(format!("AUTOMATION_DENIED: {}", stderr.trim()));
        }
        let msg = stderr.trim().to_string();
        return Err(if msg.is_empty() {
            format!("osascript exited with {}", output.status)
        } else {
            msg
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 权限体检（设置页）：主 app 与 runner 两本 TCC 账，见 tcc.rs
// ---------------------------------------------------------------------------

/// 静默检测主 app 账本的各项系统权限（零弹窗）
#[tauri::command]
pub fn check_system_permissions() -> serde_json::Value {
    serde_json::json!({
        "automationTerminal": crate::tcc::check_automation("com.apple.Terminal", false),
        "accessibility": crate::tcc::check_accessibility(),
        "screenCapture": crate::tcc::check_screen_capture(),
        "fullDiskAccess": crate::tcc::check_full_disk_access(),
    })
}

/// 主动触发系统授权弹窗（仅用户点击驱动），返回请求后的最新状态。
/// denied 记录系统不会再弹：先 tccutil reset 清掉本 app 的旧记录（等价
/// 在系统设置里删除条目——频繁构建时代的旧 DR 记录靠这个自愈），再触发
#[tauri::command]
pub async fn request_system_permission(kind: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let reset_if_denied = |service: &str, currently: &str| {
            if currently == "denied" {
                let _ = std::process::Command::new("tccutil")
                    .args(["reset", service, "com.ccspace.desktop"])
                    .output();
            }
        };
        match kind.as_str() {
            "automationTerminal" => {
                reset_if_denied(
                    "AppleEvents",
                    crate::tcc::check_automation("com.apple.Terminal", false),
                );
                // 发一个无害真实事件：目标未运行会先拉起，未决则弹授权窗
                let _ = std::process::Command::new("osascript")
                    .args(["-e", r#"tell application "Terminal" to count windows"#])
                    .output();
                Ok(crate::tcc::check_automation("com.apple.Terminal", false).to_string())
            }
            "screenCapture" => {
                reset_if_denied("ScreenCapture", crate::tcc::check_screen_capture());
                Ok(crate::tcc::request_screen_capture().to_string())
            }
            "accessibility" => {
                reset_if_denied("Accessibility", crate::tcc::check_accessibility());
                Ok(crate::tcc::prompt_accessibility().to_string())
            }
            _ => Err(format!("unsupported permission kind: {}", kind)),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 打开系统设置对应隐私面板（白名单锚点）
#[tauri::command]
pub fn open_privacy_settings(panel: String) -> Result<(), String> {
    let anchor = match panel.as_str() {
        "automation" => "Privacy_Automation",
        "accessibility" => "Privacy_Accessibility",
        "screenRecording" => "Privacy_ScreenCapture",
        "allFiles" => "Privacy_AllFiles",
        "localNetwork" => "Privacy_LocalNetwork",
        _ => return Err(format!("unknown panel: {}", panel)),
    };
    std::process::Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{}",
            anchor
        ))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// runner 账本与主 app 分离（TCC 按 responsible process 记账），必须经
/// launchd 启动才是真实语境——主 app 直接 spawn 的话归因会挂到主 app 头上
#[tauri::command]
pub async fn run_runner_health_check(
    prompt_kind: Option<String>,
) -> Result<serde_json::Value, String> {
    if cfg!(not(target_os = "macos")) {
        return Err("macOS only".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::scheduler::install_runner_binary()?;
        let runner = crate::scheduler::runner_binary_path();
        let result_path = runner_health_result_path();
        let _ = std::fs::remove_file(&result_path);

        // 与真实 routine 完全一致的启动机制（plist + bootstrap 到 gui 域）。
        // 不用 launchctl submit：submit 的 job 挂在提交者会话下，会继承
        // 主 app 的 TCC 语境，检测结果失真
        let label = "com.cc-space.health-check";
        let uid = std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "501".to_string());
        let domain = format!("gui/{}", uid);
        let service_target = format!("{}/{}", domain, label);

        // prompt 模式：runner 对指定权限发起请求式调用（弹系统授权窗）。
        // kind 走白名单防 plist 注入
        let prompt_args = match prompt_kind.as_deref() {
            Some(k @ ("automationSystemEvents" | "accessibility" | "screenCapture")) => {
                format!("\t\t<string>--prompt</string>\n\t\t<string>{}</string>\n", k)
            }
            Some(other) => return Err(format!("unsupported prompt kind: {}", other)),
            None => String::new(),
        };
        let plist_path = std::env::temp_dir().join("com.cc-space.health-check.plist");
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key><string>{}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>--health-check</string>
{}	</array>
	<key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
            label,
            runner.display(),
            prompt_args
        );
        std::fs::write(&plist_path, &plist).map_err(|e| e.to_string())?;

        let _ = std::process::Command::new("launchctl")
            .args(["bootout", &service_target])
            .output();
        let bootstrap = std::process::Command::new("launchctl")
            .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
            .output()
            .map_err(|e| e.to_string())?;
        if !bootstrap.status.success() {
            let _ = std::fs::remove_file(&plist_path);
            return Err(format!(
                "launchctl bootstrap failed: {}",
                String::from_utf8_lossy(&bootstrap.stderr).trim()
            ));
        }

        // prompt 模式会阻塞在系统授权窗上，等待时长给足用户反应时间
        let max_polls = if prompt_kind.is_some() { 600 } else { 50 };
        let mut content = None;
        for _ in 0..max_polls {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if result_path.exists() {
                // 结果文件出现后稍等写完
                std::thread::sleep(std::time::Duration::from_millis(150));
                content = std::fs::read_to_string(&result_path).ok();
                break;
            }
        }
        let _ = std::process::Command::new("launchctl")
            .args(["bootout", &service_target])
            .output();
        let _ = std::fs::remove_file(&plist_path);

        let Some(text) = content else {
            return Err("health check timed out".to_string());
        };
        serde_json::from_str(&text).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 读取上次 runner 健康检查结果（不触发新检测）
#[tauri::command]
pub fn get_runner_health_snapshot() -> Option<serde_json::Value> {
    std::fs::read_to_string(runner_health_result_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// 与 runner 侧硬编码一致：launchd 语境没有 CC_SPACE_DATA_DIR，
/// 双侧统一走默认家目录，避免 dev 环境变量导致读写错位
fn runner_health_result_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".cc-space")
        .join("permissions-runner.json")
}

/// 在 VSCode 中打开项目目录
#[tauri::command]
pub fn resume_in_vscode(cwd: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(format!("vscode://file{}", cwd))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 在 Finder 中打开目录
#[tauri::command]
pub fn open_in_finder(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 发送消息（长活进程：自动 open + stdin 写入；替代旧 per-message spawn）。
/// async + spawn_blocking：open_session 的初始化握手是阻塞 I/O，不能卡 IPC 主线程
#[tauri::command]
pub async fn start_streaming(
    app: tauri::AppHandle,
    session_id: String,
    cwd: String,
    message: String,
    model: Option<String>,
    effort: Option<String>,
    channel: Option<String>,
    advisor: bool,
    images: Option<Vec<serde_json::Value>>,
    permission_mode: Option<String>,
    append_system_prompt: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::send_message(
            &app,
            &session_id,
            &cwd,
            &message,
            model.as_deref(),
            effort.as_deref(),
            channel.as_deref(),
            advisor,
            images.as_deref(),
            permission_mode.as_deref(),
            append_system_prompt.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 中断当前回复（发 interrupt 控制消息，不杀进程）
#[tauri::command]
pub fn stop_streaming(session_id: String) -> Result<(), String> {
    streaming::interrupt_session(&session_id)
}

/// 运行时切换权限模式
#[tauri::command]
pub fn set_permission_mode(session_id: String, mode: String) -> Result<(), String> {
    streaming::set_permission_mode(&session_id, &mode)
}

/// 开关 Remote Control（进程未启动时自动连接）
#[tauri::command]
pub async fn toggle_remote_control(
    app: tauri::AppHandle,
    session_id: String,
    cwd: String,
    model: Option<String>,
    effort: Option<String>,
    channel: Option<String>,
    advisor: bool,
    enabled: bool,
    permission_mode: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::toggle_remote_control(
            &app,
            &session_id,
            &cwd,
            model.as_deref(),
            effort.as_deref(),
            channel.as_deref(),
            advisor,
            enabled,
            permission_mode.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 关闭会话进程（SIGTERM → 5s → SIGKILL）
#[tauri::command]
pub fn close_session(session_id: String) {
    streaming::close_session(&session_id);
}

/// 前端响应权限请求
///
/// `allow=true` 时透传给 claude CLI `{"behavior":"allow","updatedInput"?:...}`，
/// updated_input 用于交互工具（AskUserQuestion 答案注入等），缺省由 mcp 回填原 input
/// `allow=false` 时返回 `{"behavior":"deny","message":...}`，message 缺省为「用户拒绝」
#[tauri::command]
pub fn respond_permission(
    request_id: String,
    allow: bool,
    message: Option<String>,
    updated_input: Option<serde_json::Value>,
) -> Result<(), String> {
    let ok = PermissionService::respond(&request_id, allow, message, updated_input);
    if ok {
        Ok(())
    } else {
        Err(format!(
            "未找到 pending 权限请求，可能已超时或已被处理：{}",
            request_id
        ))
    }
}

/// CLI 全局配置摘要(~/.claude/settings.json):顶栏「默认」项展示真值用
#[derive(serde::Serialize)]
pub struct CliSettings {
    pub model: Option<String>,
    pub effort_level: Option<String>,
    pub ultracode: bool,
    pub permission_mode: Option<String>,
}

/// 读取 CLI settings.json 的模型/努力默认值。
/// settings.json 是活文件(CLI 内 /effort 等实时改写),每次调用现读现解析、
/// 绝不进程级缓存,见 docs/knowledge/pitfalls/cli-settings-live-rewrite.md
///
/// cwd 可选：传入时 permission_mode 按 Claude Code 优先级链读取
/// (Local > Project > User)，不传时只读 ~/.claude/settings.json
#[tauri::command]
pub fn get_cli_settings(cwd: Option<String>) -> CliSettings {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json");
    let json: Option<serde_json::Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let get_str = |key: &str| {
        json.as_ref()
            .and_then(|j| j.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let perm_mode = cwd.as_deref()
        .and_then(crate::streaming::resolve_default_permission_mode)
        .or_else(|| {
            json.as_ref()
                .and_then(|j| j.get("permissions"))
                .and_then(|p| p.get("defaultMode"))
                .and_then(|v| v.as_str())
                .map(String::from)
        });
    CliSettings {
        model: get_str("model"),
        effort_level: get_str("effortLevel"),
        ultracode: json
            .as_ref()
            .and_then(|j| j.get("ultracode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        permission_mode: perm_mode,
    }
}

/// 检测某会话是否有**外部** claude CLI 进程在运行。
/// 排除 CC Space 自身持有的长活进程 PID，只报告终端 `claude --resume` / VS Code 等外部进程。
/// 交互式 REPL(命令行不带 session-id)检测不到,属已知边界。
/// Windows 无 ps,Command 失败时返回 false 优雅降级。
#[tauri::command]
pub fn check_session_running(session_id: String) -> bool {
    let own_pid = crate::streaming::get_own_pid(&session_id);

    let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "pid,command"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|l| {
            if !l.contains(&session_id) || !l.contains("claude") {
                return false;
            }
            if let Some(own) = own_pid {
                let pid = l.trim().split_whitespace().next().and_then(|s| s.parse::<u32>().ok());
                if pid == Some(own) {
                    return false;
                }
            }
            true
        })
}

/// 终止指定会话的外部 CLI 进程（用户在 CC Space 点"停止"时调用）。
/// 排除 CC Space 自身持有的长活进程,只 kill 终端 / VS Code 等外部进程。
#[tauri::command]
pub fn kill_external_session(session_id: String) -> bool {
    let own_pid = crate::streaming::get_own_pid(&session_id);

    let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "pid,command"])
        .output()
    else {
        return false;
    };
    let mut killed = false;
    for l in String::from_utf8_lossy(&output.stdout).lines() {
        if !l.contains(&session_id) || !l.contains("claude") {
            continue;
        }
        let Some(pid) = l.trim().split_whitespace().next().and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if let Some(own) = own_pid {
            if pid == own {
                continue;
            }
        }
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .output();
        killed = true;
    }
    killed
}

/// 全项目 token 用量聚合（v2.2.0 FR-001）：首页 Token 卡 / 活跃热力图数据源。
/// 全量扫描秒级耗时，丢 blocking 线程池跑，不占 IPC 主线程
#[tauri::command]
pub async fn get_usage_stats() -> Result<usage_stats::UsageStats, String> {
    tauri::async_runtime::spawn_blocking(usage_stats::collect_usage_stats)
        .await
        .map_err(|e| e.to_string())?
}

/// schema-probe 全量扫描（v2.2.0 FR-004）：首页兼容性诊断卡数据源。
/// 返回结构与 CLI `--json` 输出同构（既有契约）
#[tauri::command]
pub async fn get_schema_diagnosis() -> Result<probe::Report, String> {
    tauri::async_runtime::spawn_blocking(|| probe::run_probe(None))
        .await
        .map_err(|e| e.to_string())?
}

/// 用系统默认应用打开文件(macOS: open / Windows: cmd start / Linux: xdg-open)
#[tauri::command]
pub fn open_in_default_app(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"];
const MAX_IMAGE_SIZE: u64 = 50 * 1024 * 1024;

#[derive(serde::Serialize)]
pub struct LocalImage {
    pub data: String,
    pub mime_type: String,
}

#[tauri::command]
pub fn read_local_image(path: String) -> Result<LocalImage, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(format!("文件不存在: {}", path));
    }

    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!("不支持的图片格式: {}", ext));
    }

    let meta = fs::metadata(&p).map_err(|e| e.to_string())?;
    if meta.len() > MAX_IMAGE_SIZE {
        return Err(format!(
            "图片过大: {:.1}MB（上限 {}MB）",
            meta.len() as f64 / 1_048_576.0,
            MAX_IMAGE_SIZE / 1_048_576
        ));
    }

    let mime_type = match ext.as_str() {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
    .to_string();

    let bytes = fs::read(&p).map_err(|e| e.to_string())?;
    let data = STANDARD.encode(&bytes);

    Ok(LocalImage { data, mime_type })
}

fn session_path(project_id: &str, session_id: &str) -> PathBuf {
    projects_dir()
        .join(project_id)
        .join(format!("{}.jsonl", session_id))
}

pub(crate) fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("projects")
}

// ---------- 子 Agent ----------

#[derive(serde::Serialize)]
pub struct SubAgentMeta {
    pub agent_id: String,
    pub tool_use_id: String,
    pub agent_type: Option<String>,
    pub description: Option<String>,
}

#[tauri::command]
pub fn list_subagents(project_id: String, session_id: String) -> Vec<SubAgentMeta> {
    let dir = projects_dir()
        .join(&project_id)
        .join(&session_id)
        .join("subagents");
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".meta.json") {
            continue;
        }
        let agent_id = name
            .strip_prefix("agent-")
            .and_then(|s| s.strip_suffix(".meta.json"))
            .unwrap_or("")
            .to_string();
        if agent_id.is_empty() {
            continue;
        }
        if let Ok(bytes) = fs::read(entry.path()) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                result.push(SubAgentMeta {
                    agent_id,
                    tool_use_id: v["toolUseId"].as_str().unwrap_or("").to_string(),
                    agent_type: v["agentType"].as_str().map(String::from),
                    description: v["description"].as_str().map(String::from),
                });
            }
        }
    }
    result
}

#[tauri::command]
pub fn get_subagent_records(
    project_id: String,
    session_id: String,
    agent_id: String,
) -> Vec<SessionRecord> {
    let path = projects_dir()
        .join(&project_id)
        .join(&session_id)
        .join("subagents")
        .join(format!("agent-{}.jsonl", agent_id));
    parser::parse_messages(&path)
}

#[tauri::command]
pub fn fork_session(
    source_session_id: String,
    new_session_id: String,
    cwd: String,
) -> Result<(), String> {
    let project_dir = projects_dir().join(cwd.replace('/', "-"));
    let source = project_dir.join(format!("{}.jsonl", source_session_id));
    let dest = project_dir.join(format!("{}.jsonl", new_session_id));
    if !source.exists() {
        return Err(format!("Source session not found: {}", source_session_id));
    }
    std::fs::copy(&source, &dest)
        .map_err(|e| format!("Fork failed: {}", e))?;
    Ok(())
}
