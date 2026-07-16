完美！现在我有了全面的信息。让我生成最终报告：

## 搜索结果总结

我已经完整探查了 `/Users/zz/workspace/cc-apps/cc-space-tauri/src` 中的会话对话详情页面、滚动逻辑和用户消息组件。以下是关键发现：

### 1. 会话对话详情页面组件

**文件位置**: `/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/SessionDetail.vue`（1058 行）

这是主要的会话详情容器，展示用户消息和 AI 回复。关键特性：
- 支持两种模式：`mode='archive'`（档案馆只读）和 `mode='workbench'`（工作台完整交互）
- 处理会话切换、斜杠命令、权限请求、流式状态管理
- 包含输入框、消息流和权限决策卡片

---

### 2. 滚动容器及逻辑

**滚动容器元素**（SessionDetail.vue 第 832 行）：
```html
<div
  ref="scrollContainer"
  class="flex-1 overflow-y-auto min-h-0 px-4 py-3 space-y-4 overscroll-contain relative"
  @wheel.passive="onScrollWheel"
  @scroll.passive="onScroll"
>
```
- **容器**：`scrollContainer` 是 ref 引用的 HTMLElement
- **样式**：`flex-1 overflow-y-auto` 实现垂直可滚动区域
- **事件**：
  - `@wheel.passive` 用于检测用户向上滑动（脱离跟随）
  - `@scroll.passive` 用于监测滚动方向和恢复跟随

---

### 3. 自动滚动逻辑

**逻辑实现**：内联在 SessionDetail.vue 组件中（非独立 composable）

#### 核心函数（第 549-607 行）：

**isNearBottom()** - 判断是否接近底部（60px 阈值）
```typescript
function isNearBottom(): boolean {
  const el = scrollContainer.value
  if (!el) return true
  return el.scrollHeight - el.scrollTop - el.clientHeight < 60
}
```

**onScrollWheel()** - 鼠标滚轮事件处理（第 562-564 行）
```typescript
function onScrollWheel(e: WheelEvent) {
  if (e.deltaY < 0) followStreaming.value = false  // 向上滚动时脱离跟随
}
```

**onScroll()** - 通用滚动事件处理（第 573-587 行）
```typescript
function onScroll() {
  const el = scrollContainer.value
  if (!el) return
  const delta = el.scrollTop - lastScrollTop
  lastScrollTop = el.scrollTop
  if (delta < 0) {
    followStreaming.value = false  // 向上滚动脱离
  } else if (
    delta > 0 &&
    !followStreaming.value &&
    el.scrollHeight - el.scrollTop - el.clientHeight < 120  // 120px 宽阈值恢复
  ) {
    followStreaming.value = true   // 向下滚动接近底部时恢复跟随
  }
}
```

**scrollToBottom()** - 滚动到底部（第 594-607 行）
```typescript
function scrollToBottom(force = false) {
  if (!force && !isNearBottom()) return  // 非强制时只在接近底部才滚
  nextTick(() => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const el = scrollContainer.value
        if (!el) return
        // behavior: 'instant' 在高频流式调用时保持平滑,避免被浏览器取消重启
        el.scrollTo({ top: el.scrollHeight, behavior: 'instant' })
      })
    })
  })
}
```

#### 跟随状态管理（第 560、124、526-527 行）：

```typescript
const followStreaming = ref(true)  // 初始值为 true，用户跟随流式

// 会话切换时复位（第 121-126 行）
watch(effectiveSessionId, () => {
  hideHistory.value = false
  showHelpCard.value = false
  followStreaming.value = true     // 新会话重置为跟随
  lastScrollTop = 0
})

// 发送消息时启动跟随（第 526-527 行）
followStreaming.value = true
scrollToBottom(true)
```

#### 滚动跟随 watcher（第 655-657 行）：
```typescript
// 监听全局 streamingTick（每帧递增一次），仅在流式中且用户未脱离时滚动
watch(streamingTick, () => {
  if (stream.value.streaming && followStreaming.value) scrollToBottom()
})
```

#### 历史消息变化时的滚动（第 610-612 行）：
```typescript
// 记录变化(加载/流后落账)只在跟随态回底:用户上滚阅读时不打扰
watch(records, () => {
  if (followStreaming.value) scrollToBottom(true)
})
```

---

### 4. 流式驱动滚动的核心机制

**文件位置**: `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/useStreaming.ts`

**streamingTick 全局信号**（第 159 行）：
```typescript
export const streamingTick = ref(0)
```

这是全局的流式驱动信号：
- 每当字符级 delta 到达时，`bump()` 函数在 RAF 中递增 `streamingTick`
- SessionDetail 的 `watch(streamingTick, ...)` 监听此信号
- 每帧最多播放 N 个字符（自适应速率，第 174-178 行），然后 `streamingTick++`
- 打字机效果 + 滚动跟随完全解耦

打字机批处理（第 195-246 行）：
- 字符级 delta 累积在 `pendingTextDeltas` 中
- 每帧 flush 一部分字符到 block.text
- `streamingTick++` 触发 SessionDetail 的滚动检查

---

### 5. 用户消息组件

**用户消息在 SessionDetail.vue 中的渲染**（第 849-877 行）：

不存在独立的 `UserMessage` 组件。用户消息作为消息流的一部分直接在 SessionDetail 中渲染：

```html
<template v-for="(msg, i) in messages" :key="msg.uuid || `msg-${i}`">
  <!-- system 事件行 -->
  <SystemEventRow v-if="msg.type === 'system'" :record="msg" />
  
  <!-- 用户消息和 AI 响应的统一气泡 -->
  <div v-else class="flex gap-3 msg-block">
    <div
      class="w-0.5 shrink-0 rounded-full"
      :class="msg.type === 'user' ? 'bg-primary/60' : 'bg-claude/60'"
    />
    <div class="min-w-0 flex-1">
      <div class="text-xs font-medium mb-1"
        :class="msg.type === 'user' ? 'text-primary' : 'text-claude'"
      >
        {{ msg.type === 'user' ? '你' : 'Claude' }}
      </div>
      <!-- 消息内容块渲染（用户消息也经过 contentBlocks() 解析） -->
      <div>
        <MessageBlock
          v-for="(block, i) in contentBlocks(msg)"
          :key="i"
          :block="block"
        />
      </div>
    </div>
  </div>
</template>
```

**消息块类型**（TypeScript 定义，types/index.ts 第 51-58 行）：
```typescript
export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; thinking: string; signature?: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> }
  | { type: 'tool_result'; tool_use_id: string; content: string | ContentBlock[]; is_error: boolean }
  | { type: 'image'; ... }
  | { type: 'document'; ... }
  | { type: string; [key: string]: unknown }
```

**MessageBlock.vue** 组件（/Users/zz/workspace/cc-apps/cc-space-tauri/src/components/MessageBlock.vue，19 行）：
- 动态解析 ContentBlock 类型，分发到对应的渲染组件
- 传递 `streaming` 标志给 text 块（流式中降级渲染，无高亮）

---

### 6. 重点架构要点

| 项 | 说明 |
|---|---|
| **滚动容器** | SessionDetail.vue 的 `<div ref="scrollContainer" class="overflow-y-auto">` |
| **跟随状态** | `followStreaming.value` 控制是否追踪底部 |
| **激活信号** | 全局 `streamingTick` ref（useStreaming.ts 导出） |
| **打字机节奏** | 每帧自适应播放 1-8 字符，间隔 16-80ms |
| **滚动方向检测** | wheel 事件（向上脱离）+ scroll 事件（双向判定） |
| **阈值管理** | 脱离：60px；恢复：120px + 向下滚动方向 |
| **用户消息** | 无独立组件，作为 `messages.value` 中 `msg.type === 'user'` 的记录直接渲染 |
| **块分发** | MessageBlock → resolveBlock(block.type) → BLOCK_MAP（BlockText/BlockThinking/etc） |

---

### 7. 流程关键路径

1. **发送消息** → `followStreaming = true` + `scrollToBottom(true)`
2. **流式事件** → `pendingTextDeltas` 累积 → `bump()` 定时器
3. **RAF flush** → 合并字符 + `streamingTick++`
4. **SessionDetail watch streamingTick** → `scrollToBottom()`（仅在 followStreaming=true 时）
5. **用户滚动** → `onScrollWheel()/onScroll()` 更新 `followStreaming` 状态
6. **流结束** → `finishStream()` 最后一次 flush + reload jsonl 记录