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
const { root, splitPane, closePane, setPaneSession } = useSplitLayout()

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

// --- 会话 ID 来源 ---

/** 分屏模式下从 pane state 取 sessionId */
function findPaneSessionId(): string | null {
  if (!props.paneId) return null
  return findInNode(root.value, props.paneId)
}

function findInNode(node: any, paneId: string): string | null {
  if (node.type === 'pane') return node.id === paneId ? node.sessionId : null
  if (node.type === 'split') {
    return findInNode(node.first, paneId) || findInNode(node.second, paneId)
  }
  return null
}

/** 当前面板的有效 sessionId */
const effectiveSessionId = computed(() => {
  if (props.paneId) return findPaneSessionId()
  return selectedSessionId.value
})

// 全局选中变化时同步到活跃面板
watch(selectedSessionId, (sid) => {
  if (props.paneId && sid) {
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
    (r): r is Extract<SessionRecord, { type: 'user' | 'assistant' }> =>
      r.type === 'user' || r.type === 'assistant',
  ),
)

function contentBlocks(record: Extract<SessionRecord, { type: 'user' | 'assistant' }>): ContentBlock[] {
  if (!record.message) return []
  if (record.type === 'user') {
    const content = record.message.content
    if (typeof content === 'string') {
      return [{ type: 'text', text: content }]
    }
    return content
  }
  return record.message.content
}

async function handleSend() {
  const text = inputText.value.trim()
  if (!text || !currentSession.value) return
  const cs = currentSession.value
  if (!cs.summary.cwd) return
  inputText.value = ''
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
  if (props.paneId) splitPane(props.paneId, 'horizontal')
}
function onSplitDown() {
  if (props.paneId) splitPane(props.paneId, 'vertical')
}
function onClose() {
  if (props.paneId) closePane(props.paneId)
}

// 流式结束后重新加载
watch(streaming, (val, oldVal) => {
  if (!val && oldVal) {
    reloadRecords()
  }
})

watch(
  () => streamingTurns.value.length,
  () => {
    nextTick(() => {
      if (scrollContainer.value) {
        scrollContainer.value.scrollTop = scrollContainer.value.scrollHeight
      }
    })
  },
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
              class="p-1 rounded text-default4 hover:text-default3 hover:bg-hover transition-colors"
              title="下方分屏"
              @click="onSplitDown"
            >
              <span class="i-carbon-row w-3.5 h-3.5" />
            </button>
            <button
              class="p-1 rounded text-default4 hover:text-red-400 hover:bg-red-500/10 transition-colors"
              title="关闭面板"
              @click="onClose"
            >
              <span class="i-carbon-close w-3.5 h-3.5" />
            </button>
          </template>
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
    <div v-else ref="scrollContainer" class="flex-1 overflow-y-auto min-h-0 px-4 py-3 space-y-4">
      <div
        v-for="msg in messages"
        :key="msg.uuid || Math.random()"
        class="flex gap-3"
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
          v-model="inputText"
          :disabled="streaming"
          placeholder="输入消息…"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-input border border-divider
                 text-default placeholder-default4 resize-none
                 focus:outline-none focus:border-blue-500/50 transition-colors
                 disabled:opacity-50"
          @keydown="onInputKeydown"
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
