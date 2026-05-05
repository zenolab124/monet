# 权限请求 UI(FR-003)集成指南

本文档描述如何把以下三件新增物接入主代码:

- `src/utils/dangerousOps.ts` — 危险规则识别(纯函数,无副作用)
- `src/composables/usePermissionRequests.ts` — 队列 + 事件监听 + 会话允许缓存(模块级单例)
- `src/components/PermissionCard.vue` — 单条权限卡片 UI

> 落地时按下文步骤改 `App.vue` 与 `SessionDetail.vue`,本目录下其它已有文件不动。

---

## 1. 事件契约(与 Rust 端约定)

### 1.1 Rust → Front: Tauri Event `permission-request`

```ts
{
  requestId: string                    // Rust 生成的唯一 ID
  toolName: string                     // 如 "Edit" / "Bash" / "mcp__github__create_issue"
  input: Record<string, unknown>       // 工具调用 input 对象,原样传
  timestamp: number                    // ms
}
```

发出位置:Rust 端内置 MCP server 收到 `tools/call(approve_tool_use, ...)` 时
`AppHandle::emit("permission-request", payload)`,然后阻塞等前端响应或 60s 超时。

### 1.2 Front → Rust: Tauri Command `respond_permission`

```rust
#[tauri::command]
fn respond_permission(
  request_id: String,
  allow: bool,
  message: Option<String>,
) -> Result<(), String>
```

调用约定(invoke 第二参用 camelCase 键):

```ts
await invoke('respond_permission', {
  requestId: '...',
  allow: true | false,
  message: null | '用户拒绝' | '流式已中断',
})
```

Rust 端收到响应后,把 `{ behavior: "allow" }` 或 `{ behavior: "deny", message }`
作为 MCP `tools/call` 的 result 返回给 claude CLI。

### 1.3 超时

- Rust 端独立超时(60s),前端 UI 也独立超时(60s),两边都拒绝即可。
- 前端超时拒绝由 `PermissionCard.vue` 的 setInterval 兜底,触发 `decide('deny')`。

---

## 2. App.vue:启动监听

在应用根组件挂一次,**整个 app 生命周期只能调一次**(usePermissionRequests 内部 idempotent)。

```vue
<script setup lang="ts">
import { onMounted } from 'vue'
import { useSessions } from '@/composables/useSessions'
import { initPermissionListener } from '@/composables/usePermissionRequests'

const { selectedSessionId } = useSessions()

onMounted(async () => {
  await initPermissionListener(() => selectedSessionId.value)
})
</script>
```

> `initPermissionListener` 接的是 getter 函数,而非 ref——这样在 listener 触发时才读最新值,
> 避免闭包捕获 stale 值。

---

## 3. SessionDetail.vue:渲染权限卡片

### 3.1 import

```ts
import { usePermissionRequests } from '@/composables/usePermissionRequests'
import PermissionCard from './PermissionCard.vue'

const { current: permissionRequest, respondCurrent, denyAllPending, clearSessionAllowList } =
  usePermissionRequests()
```

### 3.2 模板:固定在消息流底部、输入框上方

建议位置:在现有 `<div v-if="streamError" ...>` 之后(消息流容器内),
或者紧贴输入框上方(更靠近用户视线)。我倾向后者:

```vue
<!-- 消息流容器 -->
<div ref="scrollContainer" class="flex-1 overflow-y-auto ...">
  ...历史消息 + streamingTurns...
</div>

<!-- 权限卡片(浮在输入框上方,固定定位) -->
<div
  v-if="permissionRequest"
  class="px-4 pb-2 shrink-0 flex justify-center"
>
  <PermissionCard
    :key="permissionRequest.requestId"
    :request="permissionRequest"
    @decide="onPermissionDecide"
  />
</div>

<!-- 输入框 -->
<div v-if="..." class="px-4 py-3 border-t border-divider shrink-0 relative">
  ...
</div>
```

`:key="requestId"` 关键:队列推进时强制重建 PermissionCard,
确保倒计时重置、按钮焦点重新落到"允许一次"。

### 3.3 处理决策

```ts
async function onPermissionDecide(
  decision: 'allow_once' | 'allow_session' | 'deny',
) {
  const sid = currentSession.value?.summary.id ?? null
  await respondCurrent(decision, sid)
}
```

### 3.4 流式中断时清空 pending

在现有 `stopStreaming()` 调用前后,新增 denyAllPending:

```ts
async function onStopStreaming() {
  await denyAllPending()    // 先拒绝所有 pending,避免 Rust 端超时等 60s
  await stopStreaming()
}
```

如果中断动作只在 `useStreaming.stopStreaming` 内部,可在 SessionDetail 的
"按 Esc 中断"或"点击中断按钮"事件处理器里补一行 `await denyAllPending()`。

### 3.5 会话切换时清空 sessionAllowList(可选)

PRD L207 规定 "allow_session" 范围限当前 sessionId。当切到别的会话时,
旧 sid 缓存继续留着也无害(键里含 sid,新会话不会命中)。
但若用户在长对话中希望"换个会话就重新询问",可在会话切换 watch 里调:

```ts
watch(effectiveSessionId, () => {
  clearSessionAllowList()
})
```

> 默认建议**不清空**——sid 隔离已经够了,清空只是更保守的口味。

---

## 4. 危险规则说明

`dangerousOps.ts` 严格按 PRD L201 硬编码,**清单变更属新版 FR**,本版禁止扩展。

### 4.1 Bash 命中任一即危险

- `\brm\s+-rf?\b`         — `rm -r` / `rm -rf`
- `\bsudo\b`              — sudo
- `>\s*\/(?!tmp|var\/folders|var\/tmp)` — 重定向写入根(豁免三个临时目录)
- `\bmkfs\b`              — mkfs
- `\bdd\s+if=`            — dd
- `:\(\)\{\s*:\|:&\s*\};:` — fork bomb

### 4.2 Write/Edit/NotebookEdit 路径前缀命中即危险

- `/etc/`  `/usr/`  `/System/`  `/Library/`
- `~/.ssh/`  `~/.aws/`  `~/.gnupg/`(claude CLI 一般已展开成绝对路径,前端只做字符串前缀比对)

### 4.3 不在清单内的工具/参数一律视为非危险

包括所有 mcp_*、Read、Grep、Glob、WebFetch、WebSearch、Task、TodoWrite 等。
卡片仍会弹,但不显示红色高风险标记。

---

## 5. 边界与取舍

| 议题 | 选择 |
|---|---|
| 队列 vs 并发 | 队列(PRD L211),一次显示一个,处理完弹下一个;`current` 是 computed 队首 |
| 模块级单例 vs per-pane | 单例;app 全局只一份队列,与 `useStreaming` 当前结构对齐 |
| 倒计时双方独立 | Rust 60s + 前端 60s 各自计时,前端先到则 invoke deny;Rust 先到则 emit 一个静默清除事件(由 Rust 端设计,前端不依赖) |
| `~` 展开 | 不展开。浏览器无 process.env.HOME,直接做字符串前缀匹配。claude CLI 通常已展开成绝对路径 |
| `allow_session` 关键参数 | 取 `file_path` → `command` → `url` 中首个非空字符串。无可用关键参数时**不入缓存**(防止"模糊放行") |
| 危险态视觉 | 红色边框 + ring + warning-alt 图标 + "高风险操作"徽标 + reason 副标题。**不只靠颜色**,色盲也可感知(NFR-002) |
| 默认聚焦 | "允许一次"(蓝主按钮);PRD L205 |
| 键盘 | Enter=allow_once / Esc=deny / Tab 在三按钮循环(浏览器原生);单卡片可见时全局监听 capture 模式拦 Enter+Esc |
| `prefers-reduced-motion` | 卡片内的过渡(进度条、按钮悬浮)在 reduce 模式下禁用 |

---

## 6. 验收对照(PRD L204-L214)

| 验收点 | 实现位置 |
|---|---|
| 弹卡片含工具名/参数预览(复用 ToolEdit)/三按钮/默认聚焦"允许一次" | PermissionCard 头部 + `<component :is="resolveTool(...)" />` + onMounted focus |
| 允许一次:工具放行,后续相同请求仍弹 | respondCurrent('allow_once', sid),不写 sessionAllowList |
| 允许此会话:同 sid + toolName + 关键参数自动放行 | sessionAllowList 命中 → listener 直接 invoke allow,不入队 |
| 拒绝:CLI 收到 deny + "用户拒绝" | respondCurrent('deny', _) → invoke({allow:false, message:'用户拒绝'}) |
| 60s 无操作自动拒绝 | PermissionCard setInterval + emit('decide','deny') |
| 流式中断时所有 pending 拒绝 | denyAllPending() |
| 多个并发请求队列展示 | queue 数组,current 取队首,处理完出队 |
| 危险工具红色警示 | dangerousOps.checkDangerous + 红边/ring/图标/徽标/reason 副标题 |
| MCP server 启动失败 → 流式启动也失败 | Rust 端职责,前端不主动处理 |
| MCP server 流式期间崩溃 → 流式中断 | Rust 端职责,前端通过 stream-event error 走现有错误链路 |

---

## 7. Rust 端契约提醒(交给 R agent)

为避免前端对接卡壳,以下是必须由 Rust 端落实的前端可见接口:

1. **Tauri Event `permission-request`**:payload 字段名严格 camelCase(`requestId` / `toolName` / `input` / `timestamp`),不要用 snake_case
2. **Tauri Command `respond_permission`**:参数键名 camelCase(`requestId` / `allow` / `message`)
3. **超时回收**:Rust 端独立 60s 超时,**不需要**前端 60s 时主动调一个"取消"接口——前端 60s 后会调 `respond_permission(allow:false, message:'流式已中断')`,Rust 端按 deny 处理即可
4. **会话切换时**:Rust 端不感知"会话切换",sessionAllowList 完全在前端维护
5. **MCP 启动失败**:走现有 stream-event error 链路或 `start_streaming` invoke reject,前端不需要新事件
