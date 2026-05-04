<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { createSessionDetail } from '@/composables/useSessionDetail'
import { useStreaming } from '@/composables/useStreaming'
import { useSplitLayout } from '@/composables/useSplitLayout'
import {
  displayTitle,
  shortId,
  shortModel,
  relativeTime,
  formatTokens,
  tokenTotal,
} from '@/types'
import type { SessionRecord, SessionSummary, ContentBlock } from '@/types'
import MessageBlock from './MessageBlock.vue'
import ActionBar from './ActionBar.vue'

const props = defineProps<{
  paneId?: string
}>()

const { projects, loadProjects } = useProjects()
const { selectedSessionId, selectSession } = useSessions()
const { state, activePaneId, splitPane, closePane, setPaneSession } = useSplitLayout()

// 每个面板独立的 detail 实例
const detail = createSessionDetail()
const { records, loading, error, loadRecords, reloadRecords, clearRecords } = detail

const {
  streaming,
  streamingTurns,
  pendingUserMessage,
  streamError,
  sendMessage,
  stopStreaming,
} = useStreaming()

const inputText = ref('')
const scrollContainer = ref<HTMLElement>()
const textareaRef = ref<HTMLTextAreaElement>()

function autoResize() {
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 160) + 'px'
}

// --- 会话 ID 来源 ---

/** 分屏模式下从扁平 panes 数组取 sessionId */
function findPaneSessionId(): string | null {
  if (!props.paneId) return null
  return state.value.panes.find(p => p.id === props.paneId)?.sessionId ?? null
}

/** 当前面板的有效 sessionId */
const effectiveSessionId = computed(() => {
  if (props.paneId) return findPaneSessionId()
  return selectedSessionId.value
})

// 全局选中变化时只同步到当前活跃面板，避免所有 pane 都被改写
watch(selectedSessionId, (sid) => {
  if (props.paneId && sid && activePaneId.value === props.paneId) {
    setPaneSession(props.paneId, sid)
  }
})

// --- 会话数据 ---

function onDeleted() {
  if (props.paneId) {
    setPaneSession(props.paneId, null)
  } else {
    selectSession(null)
  }
  clearRecords()
  loadProjects()
}

const currentSession = computed<{ summary: SessionSummary; projectId: string } | null>(() => {
  const sid = effectiveSessionId.value
  if (!sid) return null
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === sid)
    if (s) return { summary: s, projectId: p.id }
  }
  return null
})

const messages = computed(() =>
  records.value.filter(
    (r): r is Extract<SessionRecord, { type: 'user' | 'assistant' }> => {
      if (r.type === 'assistant') return true
      if (r.type !== 'user') return false
      // 过滤掉纯工具结果的 user 消息（API 回传的 tool_result）
      const content = r.message?.content
      if (!content || typeof content === 'string') return true
      return content.some((b: ContentBlock) => b.type !== 'tool_result')
    },
  ),
)

/** 解析私有标签，转为特殊渲染块 */
const TAG_RE = /<(system-reminder|ide_opened_file|task-notification|user-prompt-submit-hook)[^>]*>([\s\S]*?)<\/\1>/g
const DISCARD_TAGS_RE = /<\/?(?:antml:thinking|antml:function_calls|antml:invoke|antml:parameter)[^>]*>/g

function parsePrivateTags(text: string): ContentBlock[] {
  const results: ContentBlock[] = []
  let lastIndex = 0

  // 先清理需要丢弃的标签（antml 内部标记）
  const cleaned = text.replace(DISCARD_TAGS_RE, '')

  for (const match of cleaned.matchAll(TAG_RE)) {
    const before = cleaned.slice(lastIndex, match.index).trim()
    if (before) results.push({ type: 'text', text: before })

    const [, tag, content] = match
    results.push({ type: tag, text: content.trim() } as any)

    lastIndex = match.index! + match[0].length
  }

  const after = cleaned.slice(lastIndex).trim()
  if (after) results.push({ type: 'text', text: after })

  return results
}

function contentBlocks(record: Extract<SessionRecord, { type: 'user' | 'assistant' }>): ContentBlock[] {
  if (!record.message) return []
  let blocks: ContentBlock[]
  if (record.type === 'user') {
    const content = record.message.content
    if (typeof content === 'string') {
      blocks = [{ type: 'text', text: content }]
    } else {
      blocks = content
    }
  } else {
    blocks = record.message.content
  }
  // 展开含私有标签的 text 块，检测 skill prompt
  return blocks.flatMap(b => {
    if (b.type !== 'text') return [b]
    const text = (b as any).text as string
    // Skill prompt 检测
    const skillMatch = text.match(/^Base directory for this skill:\s*(\S+)/)
    if (skillMatch) {
      const skillPath = skillMatch[1]
      const skillName = skillPath.split('/').pop() || skillPath
      return [{ type: 'skill_prompt', text: text, name: skillName } as any]
    }
    // 私有标签解析
    if (/<(?:system-reminder|ide_opened_file|task-notification|user-prompt-submit-hook)/.test(text)) {
      return parsePrivateTags(text)
    }
    return [b]
  })
}

async function handleSend() {
  const text = inputText.value.trim()
  if (!text || !currentSession.value) return
  const cs = currentSession.value
  if (!cs.summary.cwd) return
  inputText.value = ''
  if (textareaRef.value) textareaRef.value.style.height = 'auto'
  scrollToBottom(true)
  await sendMessage(cs.summary.id, cs.summary.cwd, text)
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}

// 分屏操作
function onSplitRight() {
  if (props.paneId) splitPane(props.paneId, null)
}
function onClose() {
  if (props.paneId) closePane(props.paneId)
}

/** 用户是否在底部附近（60px 阈值） */
function isNearBottom(): boolean {
  const el = scrollContainer.value
  if (!el) return true
  return el.scrollHeight - el.scrollTop - el.clientHeight < 60
}

/** 滚到底部，force=true 时无条件滚动 */
function scrollToBottom(force = false) {
  if (!force && !isNearBottom()) return
  nextTick(() => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const el = scrollContainer.value
        if (!el) return
        el.scrollTo({ top: el.scrollHeight, behavior: force ? 'instant' : 'smooth' })
      })
    })
  })
}

// 加载记录后强制滚到底部
watch(records, () => scrollToBottom(true))

// 流式结束后重新加载
watch(streaming, (val, oldVal) => {
  if (!val && oldVal) {
    reloadRecords()
  }
})

// 流式内容更新时，仅在用户停留底部时跟随
watch(
  () => streamingTurns.value.reduce((n, t) => n + t.content.length, 0),
  () => scrollToBottom(),
)

watch(
  () => currentSession.value,
  (cs) => {
    if (cs) {
      loadRecords(cs.projectId, cs.summary.id)
    } else {
      clearRecords()
    }
  },
  { immediate: true },
)
</script>

<template>
  <!-- 空态 -->
  <div v-if="!currentSession" class="h-full flex items-center justify-center">
    <p class="text-default4 text-sm">从左侧选择会话</p>
  </div>

  <div v-else class="h-full flex flex-col">
    <!-- 会话头部 -->
    <div class="px-4 py-3 border-b border-divider shrink-0">
      <div class="flex items-start justify-between gap-2">
        <h2 class="text-base font-semibold text-default truncate flex-1">
          {{ displayTitle(currentSession.summary) }}
        </h2>
        <div class="flex items-center gap-1">
          <!-- 分屏按钮（仅分屏模式下显示） -->
          <template v-if="paneId">
            <button
              class="p-1 rounded text-default4 hover:text-default3 hover:bg-hover transition-colors"
              title="右侧分屏"
              @click="onSplitRight"
            >
              <span class="i-carbon-split-screen w-3.5 h-3.5" />
            </button>
            <button
              class="p-1 rounded text-default4 hover:text-red-400 hover:bg-red-500/10 transition-colors"
              title="关闭面板"
              @click="onClose"
            >
              <span class="i-carbon-close w-3.5 h-3.5" />
            </button>
          </template>
          <button
              class="px-2 py-1 text-xs rounded-md text-default3 hover:text-default hover:bg-hover transition-colors flex items-center gap-1"
              title="刷新会话"
              @click="reloadRecords"
            >
              <span class="i-carbon-renew w-3.5 h-3.5" />
              刷新
            </button>
            <ActionBar
              :session-id="currentSession.summary.id"
              :project-id="currentSession.projectId"
              :cwd="currentSession.summary.cwd"
              @deleted="onDeleted"
            />
        </div>
      </div>
      <div class="text-xs text-default4 mt-1 flex items-center gap-2 flex-wrap">
        <span>ID: {{ shortId(currentSession.summary.id) }}</span>
        <span v-if="currentSession.summary.git_branch">
          · 分支: <span class="text-purple-400">{{ currentSession.summary.git_branch }}</span>
        </span>
        <span v-if="currentSession.summary.model">
          · 模型: {{ shortModel(currentSession.summary.model) }}
        </span>
        <span>· {{ relativeTime(currentSession.summary.last_modified) }}</span>
        <span>· {{ formatTokens(tokenTotal(currentSession.summary.total_tokens)) }} tokens</span>
      </div>
    </div>

    <!-- 加载态 -->
    <div v-if="loading" class="flex-1 flex items-center justify-center">
      <p class="text-default4 text-sm">加载对话中...</p>
    </div>

    <!-- 错误态 -->
    <div v-else-if="error" class="flex-1 flex items-center justify-center">
      <p class="text-red-400 text-sm">{{ error }}</p>
    </div>

    <!-- 无记录 -->
    <div v-else-if="messages.length === 0 && !streaming" class="flex-1 flex items-center justify-center">
      <p class="text-default4 text-sm">无对话记录</p>
    </div>

    <!-- 对话消息流 -->
    <div v-else ref="scrollContainer" class="flex-1 overflow-y-auto min-h-0 px-4 py-3 space-y-4 overscroll-contain">
      <div
        v-for="(msg, i) in messages"
        :key="msg.uuid || `msg-${i}`"
        class="flex gap-3 msg-block"
      >
        <div
          class="w-0.5 shrink-0 rounded-full"
          :class="msg.type === 'user' ? 'bg-blue-500/60' : 'bg-purple-500/60'"
        />
        <div class="min-w-0 flex-1">
          <div class="text-xs font-medium mb-1"
            :class="msg.type === 'user' ? 'text-blue-400' : 'text-purple-400'"
          >
            {{ msg.type === 'user' ? '你' : 'Claude' }}
            <span v-if="msg.type === 'assistant' && msg.message?.model" class="text-default4 font-normal">
              ({{ shortModel(msg.message.model) }})
            </span>
          </div>
          <MessageBlock
            v-for="(block, i) in contentBlocks(msg)"
            :key="i"
            :block="block"
          />
        </div>
      </div>

      <div v-if="pendingUserMessage" class="flex gap-3">
        <div class="w-0.5 shrink-0 rounded-full bg-blue-500/60" />
        <div class="min-w-0 flex-1">
          <div class="text-xs font-medium mb-1 text-blue-400">你</div>
          <div class="whitespace-pre-wrap break-words text-sm">{{ pendingUserMessage }}</div>
        </div>
      </div>

      <div v-for="turn in streamingTurns" :key="turn.messageId" class="flex gap-3">
        <div class="w-0.5 shrink-0 rounded-full bg-purple-500/60" />
        <div class="min-w-0 flex-1">
          <div class="text-xs font-medium mb-1 text-purple-400">Claude</div>
          <MessageBlock
            v-for="(block, i) in turn.content"
            :key="i"
            :block="block"
          />
        </div>
      </div>

      <div v-if="streaming && streamingTurns.length === 0" class="flex gap-3">
        <div class="w-0.5 shrink-0 rounded-full bg-purple-500/60" />
        <div class="text-xs text-default4">思考中...</div>
      </div>

      <div v-if="streamError" class="px-3 py-2 rounded-md bg-red-500/10 text-red-400 text-xs">
        {{ streamError }}
      </div>
    </div>

    <!-- 输入栏 -->
    <div v-if="currentSession.summary.cwd" class="px-4 py-3 border-t border-divider shrink-0">
      <div class="flex items-end gap-2">
        <textarea
          ref="textareaRef"
          v-model="inputText"
          :disabled="streaming"
          placeholder="输入消息… (Shift+Enter 换行)"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-input border border-divider
                 text-default placeholder-default4 resize-none
                 focus:outline-none focus:border-blue-500/50 transition-colors
                 disabled:opacity-50"
          @keydown="onInputKeydown"
          @input="autoResize"
        />
        <button
          v-if="streaming"
          class="px-3 py-2 text-xs rounded-md bg-red-500/15 text-red-400 hover:bg-red-500/25 transition-colors shrink-0"
          @click="stopStreaming"
        >
          停止
        </button>
        <button
          v-else
          :disabled="!inputText.trim()"
          class="px-3 py-2 text-xs rounded-md bg-primary/15 text-primary hover:bg-primary/25 transition-colors shrink-0
                 disabled:opacity-30 disabled:cursor-not-allowed"
          @click="handleSend"
        >
          发送
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.msg-block {
  contain: layout style;
}
</style>
