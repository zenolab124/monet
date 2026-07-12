# 斜杠命令补全（FR-004）集成指南

本文档描述如何在 `SessionDetail.vue` 中接入：

- `useSlashCommands.ts`（命令清单 + 解析）
- `SlashCommandPanel.vue`（弹出补全面板）
- `SlashHelpCard.vue`（`/help` 帮助卡片）

> 本指南只描述改动点，不包含完整代码 diff。落地时按节按行号修改即可。

---

## 1. SessionDetail.vue 改动点（精确到行号）

参考行号基于当前提交（`SessionDetail.vue` 共 418 行）。

### 1.1 `<script setup>` 顶部（约 L1-L19）

新增 import：

```ts
import SlashCommandPanel from './SlashCommandPanel.vue'
import SlashHelpCard from './SlashHelpCard.vue'
import {
  SLASH_COMMANDS,
  shouldTriggerPanel,
  parseCommand,
  type SlashCommand,
} from '@/composables/useSlashCommands'
```

### 1.2 输入态新增（紧跟 L41 `const inputText = ref('')` 后）

```ts
/** 当前光标位置（用于 shouldTriggerPanel 判定） */
const cursorPos = ref(0)

/** 命令面板是否显示 */
const slashPanelVisible = computed(() =>
  shouldTriggerPanel(inputText.value, cursorPos.value),
)

/** 校验失败提示（如 /model invalid） */
const slashError = ref<string | null>(null)

/** 前端帮助卡片显示标志（在当前 pane 渲染 SlashHelpCard） */
const showHelpCard = ref(false)
```

### 1.3 `handleSend` 改造（L167-L176）

把原本"直接 sendMessage"的链路改成先 `parseCommand`，按 kind 分发：

```ts
async function handleSend() {
  const text = inputText.value.trim()
  if (!text || !currentSession.value) return
  const cs = currentSession.value
  if (!cs.summary.cwd) return

  const parsed = parseCommand(text)

  // 解析失败的合法命令（如 /model invalid）：不清空输入，显示提示
  if (parsed.kind === 'invalid') {
    slashError.value = parsed.reason
    return
  }
  slashError.value = null

  // native 命令分发（不发给 CLI）
  if (parsed.kind === 'native') {
    handleNativeCommand(parsed.cmd, parsed.arg, cs)
    inputText.value = ''
    if (textareaRef.value) textareaRef.value.style.height = 'auto'
    return
  }

  // pass 命令（model）：持久化设置 + 清空输入；不调 sendMessage
  if (parsed.kind === 'pass' && parsed.cmd.name === 'model') {
    handleModelSwitch(parsed.arg, cs.summary.id)
    inputText.value = ''
    if (textareaRef.value) textareaRef.value.style.height = 'auto'
    return
  }

  // unknown：按普通文本发送（原始流程）
  inputText.value = ''
  if (textareaRef.value) textareaRef.value.style.height = 'auto'
  scrollToBottom(true)
  await sendMessage(cs.summary.id, cs.summary.cwd, text)
}
```

### 1.4 `onInputKeydown` 不必改

当前：

```ts
function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}
```

`SlashCommandPanel` 自己以 `capture` 模式监听 window keydown，
当面板可见且按下 ↑↓ Enter 时会 `stopPropagation + preventDefault`，
textarea 的 keydown 收不到事件，因此 `onInputKeydown` 保持不变。

但需要给 textarea 增加光标位置同步：

```vue
<textarea
  ref="textareaRef"
  v-model="inputText"
  ...
  @keydown="onInputKeydown"
  @input="onInputChange"
  @keyup="syncCursor"
  @click="syncCursor"
  @select="syncCursor"
/>
```

并新增：

```ts
function syncCursor() {
  const el = textareaRef.value
  if (el) cursorPos.value = el.selectionStart ?? 0
}

function onInputChange() {
  autoResize()
  syncCursor()
  // 用户修改输入后清掉旧的校验提示
  if (slashError.value) slashError.value = null
}
```

### 1.5 模板：在输入栏外层包一层 relative，紧邻 textarea 渲染面板（L376-L409）

```vue
<div
  v-if="currentSession.summary.cwd"
  class="px-4 py-3 border-t border-divider shrink-0 relative"
>
  <!-- 校验失败提示 -->
  <div
    v-if="slashError"
    class="mb-1 text-xs text-red-400"
  >
    {{ slashError }}
  </div>

  <SlashCommandPanel
    :visible="slashPanelVisible"
    :query="inputText"
    class="absolute bottom-full left-4 mb-1"
    @select="onSlashSelect"
    @close="onSlashClose"
  />

  <div class="flex items-end gap-2">
    <textarea ... />
    ...
  </div>
</div>
```

> 不传 `position` prop，让面板用 `absolute` 模式相对父容器定位（`bottom-full` 让它出现在输入栏正上方）。

### 1.6 模板：在消息流末尾插入 `/help` 卡片（紧跟 L370 的 streamError 之前或之后）

```vue
<SlashHelpCard
  v-if="showHelpCard"
  :commands="SLASH_COMMANDS"
/>
```

并在 `<script setup>` 暴露 `SLASH_COMMANDS` 给模板（已 import 即可）。

### 1.7 处理函数（紧跟 `onInputKeydown` 之后）

```ts
/** 用户在面板里选中某条命令：插入到输入框末尾 */
function onSlashSelect(cmd: SlashCommand) {
  // 带参数的命令补一个空格，等用户继续输入
  const insert = cmd.hasArg ? `/${cmd.name} ` : `/${cmd.name}`
  inputText.value = insert
  nextTick(() => {
    const el = textareaRef.value
    if (!el) return
    el.focus()
    const pos = insert.length
    el.setSelectionRange(pos, pos)
    cursorPos.value = pos
  })
}

function onSlashClose() {
  // 关闭面板的简单做法：清空到非触发态——这里我们仅清提示，不动输入
  // 用户继续编辑会重新触发或退出触发态
}

/** native 命令处理 */
function handleNativeCommand(
  cmd: SlashCommand,
  arg: string,
  cs: { summary: SessionSummary; projectId: string },
) {
  switch (cmd.name) {
    case 'help':
      showHelpCard.value = true
      // 滚到底部展示帮助卡片
      scrollToBottom(true)
      break
    case 'clear':
      // 需要 useStreaming 暴露 clearStreamingTurns()
      // 同时设置 hideHistory=true（前端层面隐藏会话历史消息）
      clearCurrentPaneView()
      break
    case 'new':
      // 需要 useSplitLayout 提供 newSessionInProject(projectId, cwd)
      // 临时方案：splitPane(activePaneId, null) 后等用户首次发送由 CLI 创建 sid
      handleNewSession(cs)
      break
    case 'cd':
      // 验证 arg 是已知项目路径，若是则跳转
      handleChangeDirectory(arg)
      break
  }
}

/** pass 命令处理：模型切换 */
function handleModelSwitch(modelName: string, sessionId: string) {
  // 需要 useSessionSettings.setModel(sessionId, modelName)
  // 这里只是 stub，真实集成时调用对应 composable
  // ... TODO ...
}
```

---

## 2. 命令处理路由建议

| ParsedCommand.kind | 命令     | 处理路径                                                                                                                          |
| ------------------ | -------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `unknown`          | -        | 走普通 `sendMessage(text)` 原文发送                                                                                               |
| `native`           | `/help`  | 设 `showHelpCard.value = true`，渲染 `<SlashHelpCard :commands="SLASH_COMMANDS" />`，不调 CLI                                     |
| `native`           | `/clear` | 调 `useStreaming` 新增的 `clearStreamingTurns()` + 在 `SessionDetail` 内部加 `hideHistory` 标志,模板 `v-if="!hideHistory"` 隐藏历史消息列表 |
| `native`           | `/new`   | 调 `useSplitLayout` 的 `splitPane(activePaneId, null)`(开新 pane) 或新增 `useSplitLayout.newSessionInProject(projectId)`               |
| `native`           | `/cd`    | 验证 arg 是否在 `useProjects.projects.value.map(p => p.cwd)` 里;命中则切换到该项目;不命中提示"路径未发现"                       |
| `pass`             | `/model` | 调用新增的 `useSessionSettings.setModel(sid, name)`,持久化到 `localStorage`,key 为 `monet:session-settings:<sid>`                  |
| `invalid`          | 任意     | 显示 `parsed.reason` 在输入框上方,**不清空输入**让用户修正                                                                        |

---

## 3. 现有代码需新增的 API 清单

落地时建议按以下顺序补齐：

### 3.1 `useStreaming.ts` 新增

```ts
/** 清空当前 pane 的流式渲染区（不影响磁盘 jsonl） */
function clearStreamingTurns() {
  streamingTurns.value = []
  pendingUserMessage.value = null
  streamError.value = null
}
```

并在 `useStreaming()` 返回值里 export。

> 注：当前 `streamingTurns` 是模块级单例 ref，`/clear` 会影响全部 pane。
> 如果需要严格按 pane 隔离，需要把 `useStreaming` 重构为 per-pane 实例（参考 `createSessionDetail` 模式）。这是更大的改动，建议在 PRD FR-004 范围内先用单例方案。

### 3.2 `SessionDetail.vue` 内部新增

```ts
/** /clear 时设置：仅前端层面隐藏历史消息，刷新或切换会话恢复 */
const hideHistory = ref(false)

function clearCurrentPaneView() {
  // streamingTurns 由全局 useStreaming 提供 clearStreamingTurns()
  // 历史消息：在前端层面隐藏（不删 jsonl）
  hideHistory.value = true
}

// 切换会话或刷新时复位
watch(effectiveSessionId, () => { hideHistory.value = false })
```

模板里的 `<div v-for="msg in messages">` 包一层 `v-if="!hideHistory"`。

### 3.3 `useSplitLayout.ts` 可选新增

```ts
/** 在指定项目目录下开一个无 sessionId 的新 pane,首次发消息时由 CLI 创建 sid */
function newSessionInProject(projectId: string) {
  // 实现:在 activePaneId 右侧 splitPane(null),
  // 并把新 pane 的 sessionId 设为 null,projectId 关联到 currentSession 计算属性
}
```

如果不想新增,`/new` 直接调用现有 `splitPane(activePaneId.value, null)` 即可,代价是新 pane 的 projectId 关联需要在 `SessionDetail` 内部从 currentSession 推导。

### 3.4 `useProjects.ts` 需暴露

确认 `projects.value` 中每个 project 是否有 `cwd` 字段(用于 `/cd` 路径校验)。
如果没有,需要新增或从 sessions 推导一个 `projectCwd(projectId): string | null` 工具函数。

### 3.5 全新 composable:`useSessionSettings.ts`

```ts
/**
 * 按 sessionId 持久化的会话设置(模型 / 努力等级)
 * key: monet:session-settings:<sid>
 *
 * 与 FR-006 共享同一份存储
 */
export interface SessionSettings {
  model?: 'sonnet' | 'opus' | 'haiku'
  effort?: 'low' | 'medium' | 'high' | 'xhigh' | 'max'
}

export function useSessionSettings(sid: string) {
  // load / save / setModel / setEffort
}
```

`/model <name>` 走 `setModel`,在下次启动 claude CLI 子进程时附加 `--model <name>`(此参数已在 FR-006 预研确认)。

---

## 4. 注意事项

- `SlashCommandPanel` 用 `window.addEventListener('keydown', ..., {capture: true})` 模式监听,
  因此**不需要**改 textarea 的 `@keydown` 事件链。capture 阶段先 `stopPropagation` 后,
  textarea 不会再收到 ↑↓ Enter Esc。
- `slashPanelVisible` 是 computed,基于 `inputText + cursorPos` 实时变化,不必手动开关。
- `/help` 卡片是单纯的前端渲染,不持久化,不写 jsonl;切换会话或刷新后消失。这是预期行为(同 PRD `/clear` 风格)。
- `/cd` 的"已知项目路径"判定建议放在 `SessionDetail` 内部,只读 `useProjects` 的 projects 列表。
- `parseCommand` 的错误提示文案("未知模型...""请提供目标项目路径")已写死在 `useSlashCommands.ts`,
  改文案直接改源即可。
- 触发判定 `shouldTriggerPanel` 严格按 PRD L266:`/` 必须在位置 0,且 cursor 之前内容只能是字母/连字符。
  输入框中已有内容时键入 `/` 不会触发(slice(0, cursorPos) 会包含前面的字符,正则不匹配)。

---

## 5. 验收对照（PRD L248-L258）

| 验收点                                                        | 实现位置                                                                 |
| ------------------------------------------------------------- | ------------------------------------------------------------------------ |
| 输入框为空 + `/` → 弹出含 5 命令、聚焦第一项                  | `shouldTriggerPanel("/", 1)===true`,`SlashCommandPanel` 默认 activeIndex=0 |
| 面板已弹 + ↑↓ 循环移动                                        | 面板 `onKeydown` 中 `(activeIndex + 1) % len`                            |
| Enter 选中 → 插入并光标置末(带参数补空格)                     | `onSlashSelect` 处理                                                     |
| `/h` → 只剩 `/help`                                           | `filterCommands("/h")`                                                   |
| `/help` Enter → 渲染本地帮助卡片不调 CLI                      | `handleNativeCommand` cmd.name='help'                                    |
| `/cd /path` → 校验路径,在则跳,不在则提示                      | `handleChangeDirectory`                                                  |
| `/x` → 空状态"无匹配,Enter 发送原文" + Enter 走普通发送       | 面板 filtered.length===0 显示空态,Enter 不被拦截,走 `handleSend`→unknown |
| `/` 不在行首 → 不触发                                         | `shouldTriggerPanel` 正则 `^\/[a-z\-]*$`                                 |
| Esc → 关闭面板焦点回输入框                                    | 面板 onKeydown 处理 Escape                                               |
| `/model invalid` → 提示"未知模型...",不发送                   | `parseCommand` 返回 invalid + `handleSend` 走 invalid 分支               |
