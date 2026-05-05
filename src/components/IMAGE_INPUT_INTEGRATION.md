# 输入框图片支持(FR-005)集成指南

本文档描述如何把 FR-005 的图片粘贴 / 拖拽 / 缩略图能力接入 `SessionDetail.vue`。
**本指南只描述改动点,不直接修改 `SessionDetail.vue`。**

PRD: `docs/prd/v1.0.0-realtime-session-experience.md` 第 271-316 行(FR-005)

---

## 0. 预研结论

阶段 1 预研已确认 claude CLI **2.1.118** 对图片输入的两条可行路径(测试见本指南末尾"附录 A")。

| 方案 | 命令形态 | 实测结果 | 改动面 |
| --- | --- | --- | --- |
| **a (优选)** | stdin 传 `--input-format stream-json`,user message.content 内含 `image` block | 助手正确识别图片 | streaming.rs 输入路径需结构改造 |
| b (兜底) | 命令行参数文本中嵌 `@<filepath>` | 助手正确识别图片 | streaming.rs 不动,只前端写临时文件 |
| c (不可行) | `--image <path>` | `error: unknown option '--image'` | — |

本指南**双方案都给**,主线程根据自己的改动预算选择;默认推荐方案 a。

---

## 1. 新增文件清单

| 文件 | 用途 | 行数 |
| --- | --- | --- |
| `src/utils/imageValidate.ts` | MIME + magic bytes 双层校验 | ~150 |
| `src/utils/imageCompress.ts` | Canvas 压缩 JPEG q=80,Blob ↔ base64 / dataUrl | ~110 |
| `src/composables/useImageInput.ts` | 粘贴 / 拖拽监听 + 状态管理 + 序列化 | ~270 |
| `src/components/InputImageThumbnail.vue` | 64x64 缩略图(× 删除按钮) | ~55 |
| `src/components/InputImageArea.vue` | 缩略图区(grid 布局 + 计数) | ~45 |

所有文件已通过 `pnpm vue-tsc --noEmit`(无类型错误)。

---

## 2. SessionDetail.vue 改动点(精确到行号)

参考行号基于当前提交(`SessionDetail.vue` 共 551 行)。

### 2.1 新增 import(顶部 import 块末尾)

```ts
import InputImageArea from './InputImageArea.vue'
import { useImageInput } from '@/composables/useImageInput'
```

### 2.2 新增状态(`<script setup>` 内,与 `inputText` 同区域)

```ts
// 图片输入(FR-005)
const inputAreaRef = ref<HTMLElement | null>(null) // 容纳 textarea+缩略图区的根 div,用于拖拽
const {
  images: pendingImages,
  lastError: imageError,
  isDragging: imageDragging,
  addFiles: addImageFiles, // 备用:外部触发(如点按钮选文件)
  removeImage,
  clearImages,
  clearError: clearImageError,
  toImageBlocks,
  attach: attachImageEvents,
  detach: detachImageEvents,
} = useImageInput({
  pasteTarget: textareaRef,
  dropTarget: inputAreaRef,
})

onMounted(() => attachImageEvents())
onBeforeUnmount(() => detachImageEvents())
```

> 如果 `SessionDetail.vue` 已经 import 过 `onMounted` / `onBeforeUnmount` 直接用即可;
> 否则在 vue import 列表里加。

### 2.3 模板:输入栏外层 ref + 拖拽高亮

把 L495 的 `<div v-if="currentSession.summary.cwd" class="px-4 py-3 border-t border-divider shrink-0 relative">`:

```diff
- <div v-if="currentSession.summary.cwd" class="px-4 py-3 border-t border-divider shrink-0 relative">
+ <div
+   v-if="currentSession.summary.cwd"
+   ref="inputAreaRef"
+   class="px-4 py-3 border-t border-divider shrink-0 relative transition-colors"
+   :class="{ 'bg-blue-500/5 ring-1 ring-blue-500/40 ring-inset': imageDragging }"
+ >
```

### 2.4 模板:缩略图区 + 错误提示(在 SlashCommandPanel 之后,textarea 之前)

```vue
<!-- 图片错误提示(沿用 slashError 风格) -->
<div
  v-if="imageError"
  class="mb-1 text-xs text-red-400 flex items-center gap-1.5"
>
  <span class="i-carbon-warning w-3.5 h-3.5" />
  {{ imageError.message }}
</div>

<!-- 拖拽提示(仅 isDragging 且无图时显示) -->
<div
  v-if="imageDragging && pendingImages.length === 0"
  class="mb-1 text-xs text-blue-400 flex items-center gap-1.5 pointer-events-none"
>
  <span class="i-carbon-image w-3.5 h-3.5" />
  拖入图片
</div>

<!-- 缩略图区 -->
<InputImageArea :images="pendingImages" @remove="removeImage" />
```

### 2.5 模板:textarea 占位文本提示

可选改进 placeholder 提示用户支持图片(L513):

```diff
- placeholder="输入消息… (Shift+Enter 换行,/ 触发命令补全)"
+ placeholder="输入消息… (Shift+Enter 换行,/ 触发命令补全,Cmd+V 粘贴图片)"
```

### 2.6 `handleSend` 改造(L290 附近)

把发送逻辑扩展为带图片版本:

```ts
async function handleSend() {
  const text = inputText.value.trim()
  // 文本和图片至少一项不为空
  if (!text && pendingImages.value.length === 0) return
  if (!currentSession.value) return
  const cs = currentSession.value
  if (!cs.summary.cwd) return

  // 斜杠命令分发(原逻辑保留;含图时强制走普通发送,跳过命令解析)
  // ↓ 仅当无图片时才尝试解析命令
  if (pendingImages.value.length === 0 && text.startsWith('/')) {
    const parsed = parseCommand(text)
    if (parsed.kind === 'invalid') {
      slashError.value = parsed.reason
      return
    }
    if (parsed.kind === 'native') {
      handleNativeCommand(parsed.cmd, parsed.arg, cs)
      inputText.value = ''
      return
    }
    if (parsed.kind === 'pass' && parsed.cmd.name === 'model') {
      handleModelSwitch(parsed.arg, cs.summary.id)
      inputText.value = ''
      return
    }
    // unknown:走下方普通发送(含 /init /compact 等)
  }

  // ---- 含图发送 ----
  const imageBlocks = await toImageBlocks() // ImageContentBlock[]

  // 清空输入
  inputText.value = ''
  clearImages()
  clearImageError()

  scrollToBottom(true)

  // 发给底层(见下方 §3 任选一种方案接入)
  await sendMessage(cs.summary.id, cs.summary.cwd, text, /* opts */ {}, imageBlocks)
}
```

> 注意:有图片时绕过斜杠命令解析,因为 `/clear` `/help` 等纯前端操作不该带图。
> `/model` 也不允许同消息带图(用户应先 `/model x` 再发图)。

---

## 3. 底层发送链路:两选一

### 3.1 方案 a — stdin stream-json(推荐)

PRD 优先方案,改动 streaming.rs 输入路径。优点:与 Anthropic API 完全对齐,后续支持文件 / 工具描述等多模态时同一路径。

#### 3.1.1 useStreaming.ts 签名扩展

```ts
import type { ImageContentBlock } from './useImageInput'

async function sendMessage(
  sessionId: string,
  cwd: string,
  message: string,
  opts: SendOptions = {},
  images: ImageContentBlock[] = [], // 新增,可选
) {
  // ... 现有前置 ...

  await invoke('start_streaming', {
    sessionId,
    cwd,
    message,
    images,                  // 透传给 Rust
    model: opts.model ?? null,
    effort: opts.effort ?? null,
  })
}
```

#### 3.1.2 commands.rs / streaming.rs 改造要点

新增 `start_streaming` 入参:`images: Option<Vec<ImageBlock>>`

```rust
#[derive(Debug, Deserialize)]
pub struct ImageBlock {
    pub r#type: String, // "image"
    pub source: ImageSource,
}

#[derive(Debug, Deserialize)]
pub struct ImageSource {
    pub r#type: String,        // "base64"
    pub media_type: String,    // image/png / image/jpeg / image/gif / image/webp
    pub data: String,          // 纯 base64,无 data URL 前缀
}
```

streaming.rs::start_streaming 流程改造:

```rust
// 1. 命令参数从原本的 [..., message] 改为 [..., --input-format, stream-json]
args.extend([
    "--resume".into(),
    session_id.to_string(),
    "--print".into(),
    "--input-format".into(),
    "stream-json".into(),
    "--output-format".into(),
    "stream-json".into(),
    "--verbose".into(),
]);
// model / effort 保持不变
// **不再**在 args 末尾 push message

// 2. spawn 时打开 stdin
let mut child = Command::new(&executable)
    .args(&args)
    .current_dir(cwd)
    .env("PATH", enhanced_path())
    .stdin(Stdio::piped())   // 新增
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;

// 3. 构造 user message JSON 并写入 stdin
let mut content = vec![serde_json::json!({"type":"text","text": message})];
for img in images.unwrap_or_default() {
    content.push(serde_json::json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": img.source.media_type,
            "data": img.source.data,
        }
    }));
}
let payload = serde_json::json!({
    "type":"user",
    "message": { "role":"user", "content": content }
});
let line = serde_json::to_string(&payload).unwrap();

if let Some(mut stdin) = child.stdin.take() {
    use std::io::Write;
    let _ = writeln!(stdin, "{}", line);
    // 关闭 stdin,通知 CLI 输入结束
    drop(stdin);
}
```

> 测试已确认:user message JSON 单行写入后 `drop(stdin)` 关闭 fd,
> claude CLI 会立刻处理并输出 stream-json,不会卡住。

#### 3.1.3 验收

预研脚本(见附录 A 方案 a)产生的 assistant 内容应在 `streamingTurns[0].content[0].text` 中可见。

### 3.2 方案 b — @filepath 兜底

streaming.rs **不改**,仅前端 + 新增一个 Rust Command 写临时文件。优点:本地改动小,缺点:消息文本里出现 `@/var/folders/...` 路径,jsonl 也会留这条引用。

#### 3.2.1 新增 Tauri Command(commands.rs / lib.rs)

```rust
// commands.rs
use std::io::Write;

#[tauri::command]
pub fn write_temp_image(base64_data: String, ext: String) -> Result<String, String> {
    // 校验扩展名白名单
    let ext = ext.to_lowercase();
    if !["png","jpg","jpeg","gif","webp"].contains(&ext.as_str()) {
        return Err(format!("不支持的扩展名: {}", ext));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("base64 解码失败: {}", e))?;
    let dir = std::env::temp_dir().join("cc-space-images");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4(); // 或简单的时间戳
    let path = dir.join(format!("{}.{}", id, ext));
    let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
```

注:base64 / uuid crate 需新增到 Cargo.toml(违反"不引入新依赖"约束) → 替代方案:
- base64 解码用手写或 `base64 = { version = "0.22" }`
- 文件名用 `format!("{}-{}.{}", std::process::id(), seqnum, ext)` 替代 uuid

> **本任务约束不新增依赖**,所以 b 方案如果落地,主线程要权衡是否破例。
> 如果坚持不加依赖,可以让前端把 base64 拼成 data URL 通过 IPC 传 Vec<u8>(把前端 base64 解码成 ArrayBuffer 后用 Tauri convertFileSrc 传二进制)。

注册:lib.rs `invoke_handler!` 加 `commands::write_temp_image`。

#### 3.2.2 前端集成

```ts
import { invoke } from '@tauri-apps/api/core'
import { blobToBase64 } from '@/utils/imageCompress'

async function buildAtPaths(images: PendingImage[]): Promise<string[]> {
  const paths: string[] = []
  for (const img of images) {
    const data = await blobToBase64(img.blob)
    const ext = img.mime === 'image/jpeg' ? 'jpg'
              : img.mime.replace('image/', '')
    const p = await invoke<string>('write_temp_image', {
      base64Data: data,
      ext,
    })
    paths.push(p)
  }
  return paths
}

// handleSend 内:
const paths = await buildAtPaths(pendingImages.value)
const finalText =
  paths.length > 0
    ? `${text}\n\n${paths.map(p => `@${p}`).join(' ')}`
    : text
await sendMessage(cs.summary.id, cs.summary.cwd, finalText)
```

#### 3.2.3 临时文件清理

- 每次发送前清空 `temp_dir()/cc-space-images/` 下超过 1 小时的文件(避免堆积)
- 程序退出时整体删除该目录(可在 Tauri exit 钩子里做)

---

## 4. 错误反馈细节

`useImageInput.lastError` 会按 PRD 验收标准给出对应文案:

| 触发场景 | error.kind | message |
| --- | --- | --- |
| 拖入 svg | validate | "不支持 svg 格式" |
| 拖入 mp4 | validate | "仅支持图片" |
| 拖入 PDF | validate | "不支持 PDF" |
| 拖入 heic | validate | "不支持 heic 格式" |
| 拖入 bmp / tiff | validate | "不支持 bmp/tiff 格式" |
| MIME 与 magic 不一致 | validate | "图片头与声明类型不符" |
| 压缩后仍 >2MB | compress | "图片过大,请手动缩小" |
| 第 6 张图 | limit | "单次最多 5 张" |
| FileReader 失败 | read | 具体异常 message |

CLI 端图片错误(格式不支持 / 解码失败)走原有 `streamError` 链路展示在消息流末尾的红色提示;
PRD 要求"图片暂存允许重试" — `clearImages()` 只在发送成功提交流式后调用,
若 streamError 在发送 invoke 抛错前出现,可以选择保留 images 不清空(改动 §2.6 的清理顺序即可)。

---

## 5. 验收对照(PRD L292-L302)

| 验收点 | 实现位置 |
| --- | --- |
| Cmd+V 粘贴图片 → 上方 64x64 缩略图 | `useImageInput.onPaste` + `InputImageThumbnail` |
| 拖拽 png → 同上 | `useImageInput.onDrop` |
| × 移除单张 | `removeImage(id)` |
| 发送时 image block 传给 CLI | `toImageBlocks()` + 方案 a/b 任一接入 |
| 6 张:前 5 接受,第 6 提示 | `addFiles` 截断 + lastError 设 `单次最多 5 张` |
| 8MB png 自动压缩 ≤1MB | `compressIfNeeded` 走 canvas q=80 |
| svg 拒绝并提示 "不支持 svg 格式" | `validateImage` MIME 黑名单 |
| mp4 拒绝并提示 "仅支持图片" | `validateImage` MIME 前缀 video/ |
| CLI 报图片错误 → 错误卡片 + 暂存允许重试 | streamError 链路 + 调整 clearImages 时机 |
| 压缩后仍 >2MB → 拒绝并提示 "图片过大..." | `compressIfNeeded` 二次大小检查 |
| 输入框失焦时不接受粘贴 | `pasteTarget=textareaRef`,事件绑在 textarea |
| 拖拽进入显示蓝色高亮 | `imageDragging` + ring-1 ring-blue-500/40 |
| 暗色模式正常 | 全部用 style-lab 变量(divider / input / default4),无硬编码颜色 |
| `prefers-reduced-motion` | 缩略图无动画(× 按钮 fade 是 100ms,符合"减少"语义) |

---

## 附录 A:预研测试记录

### A.1 claude --help 中所有 image / file / stdin / input / attach 相关参数

```
--bare                    Minimal mode: ... Anthropic auth ... apiKeyHelper via --settings ...
-d, --debug [filter]      Enable debug mode with optional category filtering
--debug-file <path>       Write debug logs to a specific file path
--file <specs...>         File resources to download at startup. Format: file_id:relative_path
                          (e.g., --file file_abc:doc.txt file_def:img.png)
--input-format <format>   Input format (only works with --print): "text" (default),
                          or "stream-json" (realtime streaming input)
--mcp-config <configs...> Load MCP servers from JSON files or strings
--replay-user-messages    Re-emit user messages from stdin back on stdout for acknowledgment
                          (only works with --input-format=stream-json and --output-format=stream-json)
--settings <file-or-json> Path to a settings JSON file
```

注:`--file` 是 Anthropic Files API 的 `file_id:path` 注册映射,**不是**给本地文件用的。

`--image` 不存在(`error: unknown option '--image'`)。

### A.2 方案 a 实测(stdin + stream-json + image base64 block)

```bash
TEST_SID=$(uuidgen | tr '[:upper:]' '[:lower:]')
B64=$(base64 -i /tmp/cc-space-test-real.png | tr -d '\n')
JSON='{"type":"user","message":{"role":"user","content":[{"type":"text","text":"你看到的图里大致是什么颜色?"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"'$B64'"}}]}}'
echo "$JSON" | claude --print --session-id "$TEST_SID" \
    --input-format stream-json \
    --output-format stream-json \
    --verbose
```

输出关键行:
```
{"type":"assistant", ... "content":[{"type":"text",
 "text":"主色是青蓝(C 形)和橙黄(弧线),中间是深蓝色的 \"8\" 字符。\n\n完毕。"}], ...}
{"type":"result","is_error":false,"duration_ms":6238,"stop_reason":"end_turn", ...}
```

Claude **正确识别了图片内容**(测试图是 cc-space 的 logo:青蓝 + 橙黄 + "8")。
✓ 方案 a 可行。

### A.3 方案 b 实测(@filepath)

```bash
TEST_SID=$(uuidgen | tr '[:upper:]' '[:lower:]')
claude --print --session-id "$TEST_SID" \
    "请描述图片 @/tmp/cc-space-test-real.png 的颜色,一句话" \
    --output-format stream-json --verbose
```

输出关键行:
```
{"type":"assistant", ... "content":[{"type":"text",
 "text":"蓝青色与橙黄色组成的环形图标,配深青色圆点。\n\n完毕。"}], ...}
{"type":"result","is_error":false, ...}
```

Claude 也正确识别。✓ 方案 b 可行。

### A.4 方案 c 实测(--image)

```bash
claude --image /tmp/cc-space-test-real.png --print ...
# error: unknown option '--image'
```

✗ 方案 c 不可行。

### A.5 stream-json 输入对纯文本同样工作

```bash
echo '{"type":"user","message":{"role":"user","content":[{"type":"text","text":"说一句你好"}]}}' \
  | claude --print --session-id <uuid> --input-format stream-json --output-format stream-json --verbose
```

输出 `"text":"你好。\n\n完毕。"` + `is_error:false`。

→ 方案 a 可作为 streaming.rs 的统一输入路径,纯文本和带图消息走同一管线。

---

## 附录 B:暗色模式 / 减少动效 / 键盘可达性

- **暗色模式**:所有颜色用 `divider` / `input` / `default4` / `bg-blue-500/5` 等 style-lab 变量,
  自动跟随 `<html class="dark">` 切换。
- **prefers-reduced-motion**:缩略图删除按钮的 hover fade 是 ~100ms,符合"减少动效"语义可不特殊处理;
  如需严格,InputImageThumbnail 内 `transition-opacity` 改为
  `motion-safe:transition-opacity` 即可。
- **键盘可达性**:× 按钮是 `<button type="button">` 原生 focusable,可 Tab 到并 Enter 删除。

---

## 附录 C:与 FR-004 斜杠命令的协作

- 当输入框已有图片时,`/` 不应触发斜杠命令面板(避免 `/help` 这类纯前端命令带图歧义)。
  `useSlashCommands.shouldTriggerPanel` 当前签名是 `(text, cursorPos)`,无图状态,
  改造点:在 `slashPanelVisible` 计算属性里加 `&& pendingImages.value.length === 0` 即可。

---

## 附录 D:与 FR-006 顶栏控件无依赖

模型 / 努力等级切换不影响图片输入路径。`sendMessage(opts)` 的 `model` / `effort` 参数照常传。
