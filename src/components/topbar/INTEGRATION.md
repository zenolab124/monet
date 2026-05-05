# SessionTopBar 集成指南

FR-006(会话顶栏增强)新组件的集成步骤。所有引用都按 `src/components/SessionDetail.vue`(2026-05-05 版本)的行号给出。

---

## 1. SessionDetail.vue 替换范围

### 1.1 删除的代码块

**L251-L301**(整个会话头部 `<div class="px-4 py-3 border-b border-divider shrink-0">` 到关闭的 `</div>`):

```html
<!-- 会话头部 -->
<div class="px-4 py-3 border-b border-divider shrink-0">
  <div class="flex items-start justify-between gap-2">
    <h2 class="text-base font-semibold text-default truncate flex-1">
      {{ displayTitle(currentSession.summary) }}
    </h2>
    <div class="flex items-center gap-1">
      ... 分屏/关闭/刷新/ActionBar ...
    </div>
  </div>
  <div class="text-xs text-default4 mt-1 flex items-center gap-2 flex-wrap">
    <span>ID: {{ shortId(currentSession.summary.id) }}</span>
    ... 元数据 ...
  </div>
</div>
```

整段替换为单个 `<SessionTopBar>` 组件(见 1.3)。

### 1.2 import 增量

在文件顶部 import 块(L1-L18)末尾追加:

```ts
import SessionTopBar from './topbar/SessionTopBar.vue'
import { useSessionSettings, type EffortLevel } from '@/composables/useSessionSettings'
```

同时,以下 import 在头部替换后将变为未使用,可移除:

- L9 `displayTitle`(模板里只在头部用)→ 仍在 `currentSession` 显示标题判定时不直接需要,可放心删
- L11 `shortModel` → 仍在 L296、L335 使用,**保留**
- L12 `relativeTime` → 仅 L298 头部使用,删
- L13 `formatTokens` / L14 `tokenTotal` → 仅 L299 头部使用,删

最稳的做法:先把头部替换完,然后让 vue-tsc 提示哪些 import 未用,逐个删。

### 1.3 在原头部位置插入

```vue
<SessionTopBar
  :title="displayTitle(currentSession.summary)"
  :session-id="currentSession.summary.id"
  :short-id-value="shortId(currentSession.summary.id)"
  :project-id="currentSession.projectId"
  :cwd="currentSession.summary.cwd"
  :git-branch="currentSession.summary.git_branch"
  :model-string="currentSession.summary.model"
  :total-tokens="currentSession.summary.total_tokens"
  :last-modified="currentSession.summary.last_modified"
  :selected-model-id="settings.modelId"
  :selected-effort="settings.effort"
  :show-split="!!paneId"
  @model-change="onModelChange"
  @effort-change="onEffortChange"
  @split-right="onSplitRight"
  @close="onClose"
  @reload="reloadRecords"
  @deleted="onDeleted"
/>
```

### 1.4 新增 setup 逻辑

在 L26 之后(`createSessionDetail` 调用旁)追加:

```ts
// 会话级设置(模型/努力等级),按 sessionId 持久化
const { settings, setModel, setEffort } = useSessionSettings(
  computed(() => effectiveSessionId.value),
)

function onModelChange(modelId: string) {
  setModel(modelId)
}
function onEffortChange(effort: EffortLevel) {
  setEffort(effort)
}
```

注意 `effectiveSessionId` 已在 L61 定义,`useSessionSettings` 接受 `Ref<string | null>`,直接传 `effectiveSessionId` 也可(它本身就是 `ComputedRef<string | null>`),不需要再包一层 `computed`。

### 1.5 模板引用收尾

`shortModel` 在 L334-L336 助手消息行内仍要用;`displayTitle` / `shortId` / `formatTokens` / `tokenTotal` / `relativeTime` 都被移到 `<SessionTopBar>` 内部,但 `displayTitle(currentSession.summary)` 还要传给 `:title`,所以 `displayTitle` 和 `shortId` 必须保留 import。

---

## 2. useStreaming 改动建议(只列出,不在本任务实现)

### 2.1 当前签名

```ts
// src/composables/useStreaming.ts:29
async function sendMessage(sessionId: string, cwd: string, message: string)
```

### 2.2 建议新签名

```ts
async function sendMessage(
  sessionId: string,
  cwd: string,
  message: string,
  opts?: { model?: string; effort?: EffortLevel },
)
```

`opts` 全部可选,缺省时不向 CLI 附加 `--model` / `--effort`。

### 2.3 Rust 命令改动建议

`invoke('start_streaming', { sessionId, cwd, message })`(useStreaming L78)需扩参:

```ts
await invoke('start_streaming', {
  sessionId,
  cwd,
  message,
  model: opts?.model ?? null,
  effort: opts?.effort ?? null,
})
```

Rust 侧 `start_streaming` 命令需新增两个 `Option<String>` 参数,启动 claude CLI 时:

- `model` 非空 → 追加 `--model <model>`
- `effort` 非空 → 追加 `--effort <effort>`

### 2.4 SessionDetail 调用点改动

`handleSend()`(SessionDetail.vue L167-L176)末尾:

```ts
await sendMessage(cs.summary.id, cs.summary.cwd, text, {
  model: settings.value.modelId ?? undefined,
  effort: settings.value.effort,
})
```

注意 `settings` 是 `ComputedRef<SessionSettings>`,要 `.value.modelId` / `.value.effort`。

---

## 3. 现有代码改动清单(本任务不实现,仅列出)

按优先级排:

1. **`src/components/SessionDetail.vue`** — 替换 L251-L301 头部、新增 import、新增 `useSessionSettings` 调用、新增 `onModelChange` / `onEffortChange` 函数、`handleSend` 改为带 opts
2. **`src/composables/useStreaming.ts`** — `sendMessage` 签名扩展为接收 `opts`,`invoke('start_streaming', ...)` 加 `model` / `effort` 字段
3. **`src-tauri/src/`(命令层)** — `start_streaming` Rust 命令新增 `model: Option<String>` / `effort: Option<String>` 参数;启动 claude CLI 子进程时按需追加 `--model` / `--effort` 参数
4. **(可选)`src/composables/useSessionDetail.ts`** — 若希望模型切换后强制刷新元数据展示,可考虑在切换后触发一次 reload;但 PRD 要求"流式中不中断,仅影响下次发送",所以默认不需要

---

## 4. 已知取舍

- **溢出菜单的容器宽度阈值**:用 480 / 380 / 280 三档,基于按钮+文本估算。若实际窄分屏场景下还有溢出,需要回到 `SessionTopBar.vue` 的 `showEffort` / `showProgress` 阈值微调
- **元数据中的"模型"字段去重**:当 `inferModel` 能识别 `modelString` 时(命中 sonnet/opus/haiku),已经在 `<ModelDropdown>` 显示,因此在右侧元数据里不再重复"· 模型: ..."。只有解析失败(`!effectiveModel`)的老会话才会在元数据里 fallback 显示原 `shortModel(modelString)`
- **token 进度的容量**:`selectedModelId` 优先,其次 `modelString`;两者都为空时回退用 sonnet 容量(200000),避免除 0
- **`useSessionSettings` 持久化粒度**:目前用 `watch(internal, ..., { deep: true })`,每次 setModel/setEffort 都会触发一次写入。频次低,不做节流
