<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { createSessionDetail } from '@/composables/useSessionDetail'
import { useStreaming } from '@/composables/useStreaming'
import { useSplitLayout } from '@/composables/useSplitLayout'
import { useSessionSettings } from '@/composables/useSessionSettings'
import {
  SLASH_COMMANDS,
  shouldTriggerPanel,
  parseCommand,
  type SlashCommand,
} from '@/composables/useSlashCommands'
import { displayTitle, shortId, shortModel } from '@/types'
import type { SessionRecord, SessionSummary, ContentBlock } from '@/types'
import MessageBlock from './MessageBlock.vue'
import SessionTopBar from './topbar/SessionTopBar.vue'
import SlashCommandPanel from './SlashCommandPanel.vue'
import SlashHelpCard from './SlashHelpCard.vue'
import PermissionCard from './PermissionCard.vue'
import { usePermissionRequests } from '@/composables/usePermissionRequests'

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
  clearStreamingTurns,
} = useStreaming()

const inputText = ref('')
const scrollContainer = ref<HTMLElement>()
const textareaRef = ref<HTMLTextAreaElement>()

// --- 斜杠命令(FR-004)状态 ---

/** 当前输入框光标位置(用于 shouldTriggerPanel 判定) */
const cursorPos = ref(0)

/** /model invalid 等校验失败提示 */
const slashError = ref<string | null>(null)

/** /help 帮助卡片显示标志(前端层面,不写 jsonl) */
const showHelpCard = ref(false)

/** /clear 时设置:仅前端层面隐藏历史消息,刷新或切换会话恢复 */
const hideHistory = ref(false)

const slashPanelVisible = computed(() =>
  shouldTriggerPanel(inputText.value, cursorPos.value),
)

function autoResize() {
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 160) + 'px'
}

function syncCursor() {
  const el = textareaRef.value
  if (el) cursorPos.value = el.selectionStart ?? 0
}

function onInputChange() {
  autoResize()
  syncCursor()
  if (slashError.value) slashError.value = null
}

// --- 会话 ID 来源 ---

function findPaneSessionId(): string | null {
  if (!props.paneId) return null
  return state.value.panes.find(p => p.id === props.paneId)?.sessionId ?? null
}

const effectiveSessionId = computed(() => {
  if (props.paneId) return findPaneSessionId()
  return selectedSessionId.value
})

// 全局选中变化时只同步到当前活跃面板
watch(selectedSessionId, (sid) => {
  if (props.paneId && sid && activePaneId.value === props.paneId) {
    setPaneSession(props.paneId, sid)
  }
})

// 会话切换时复位 /clear 与 /help 的视图标志,并清流式区(避免上一会话残留)
watch(effectiveSessionId, () => {
  hideHistory.value = false
  showHelpCard.value = false
  clearStreamingTurns()
})

// --- 权限请求(FR-003) ---
const { current: permissionRequest, respondCurrent, denyAllPending } = usePermissionRequests()

async function onPermissionDecide(decision: 'allow_once' | 'allow_session' | 'deny') {
  const sid = currentSession.value?.summary.id ?? null
  await respondCurrent(decision, sid)
}

async function onStopStreaming() {
  await denyAllPending()
  await stopStreaming()
}

// --- 会话级设置(FR-006):模型 / 努力等级 ---
const { settings, setModel, setEffort } = useSessionSettings(effectiveSessionId)

function onModelChange(modelId: string) {
  setModel(modelId)
}

function onEffortChange(effort: 'low' | 'medium' | 'high' | 'xhigh' | 'max') {
  setEffort(effort)
}

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

/** 最近一条 assistant 记录(用于推断真实模型与上下文占用) */
const lastAssistantRecord = computed(() => {
  for (let i = records.value.length - 1; i >= 0; i--) {
    const r = records.value[i]
    if (r.type === 'assistant' && r.message) return r
  }
  return null
})

/** 最近 assistant 跑过的真实 model 字符串(含 [1m] 后缀如有);为空时 fallback 到 summary.model */
const lastAssistantModel = computed<string | null>(() => {
  return lastAssistantRecord.value?.message?.model ?? null
})

/**
 * 已占用上下文 token 数 = 最近一次 assistant 响应的 input_tokens + cache_read_input_tokens。
 *
 * 这两项之和反映"模型这次实际看到的 prompt token 总量",是"下次请求即将占用上下文容量"
 * 的真实近似。不能用 SessionSummary.total_tokens 累加 4 类——那是计费统计量,
 * cache_read 累计会让长会话出现几十 M 的虚高。
 */
const lastAssistantContextSize = computed<number>(() => {
  const u = lastAssistantRecord.value?.message?.usage
  if (!u) return 0
  return u.input_tokens + u.cache_read_input_tokens
})

/** 流式区当前渲染的 message id 集合(用于历史区过滤,避免与流式区重复显示) */
const streamingMessageIds = computed(() =>
  new Set(streamingTurns.value.map(t => t.messageId)),
)

const messages = computed(() =>
  records.value.filter(
    (r): r is Extract<SessionRecord, { type: 'user' | 'assistant' }> => {
      if (r.type === 'assistant') {
        // 当前流式区还在显示这条消息时,历史区跳过(避免双显示)
        const msgId = r.message?.id
        if (msgId && streamingMessageIds.value.has(msgId)) return false
        return true
      }
      if (r.type !== 'user') return false
      const content = r.message?.content
      if (!content || typeof content === 'string') return true
      return content.some((b: ContentBlock) => b.type !== 'tool_result')
    },
  ),
)

/** 解析私有标签,转为特殊渲染块 */
const TAG_RE = /<(system-reminder|ide_opened_file|task-notification|user-prompt-submit-hook)[^>]*>([\s\S]*?)<\/\1>/g
const DISCARD_TAGS_RE = /<\/?(?:antml:thinking|antml:function_calls|antml:invoke|antml:parameter)[^>]*>/g

function parsePrivateTags(text: string): ContentBlock[] {
  const results: ContentBlock[] = []
  let lastIndex = 0
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
  return blocks.flatMap(b => {
    if (b.type !== 'text') return [b]
    const text = (b as any).text as string
    const skillMatch = text.match(/^Base directory for this skill:\s*(\S+)/)
    if (skillMatch) {
      const skillPath = skillMatch[1]
      const skillName = skillPath.split('/').pop() || skillPath
      return [{ type: 'skill_prompt', text: text, name: skillName } as any]
    }
    if (/<(?:system-reminder|ide_opened_file|task-notification|user-prompt-submit-hook)/.test(text)) {
      return parsePrivateTags(text)
    }
    return [b]
  })
}

// --- 斜杠命令处理 ---

function onSlashSelect(cmd: SlashCommand) {
  const insert = cmd.hasArg ? `/${cmd.name} ` : `/${cmd.name}`
  inputText.value = insert
  nextTick(() => {
    const el = textareaRef.value
    if (!el) return
    el.focus()
    autoResize()
    const pos = insert.length
    el.setSelectionRange(pos, pos)
    cursorPos.value = pos
  })
}

function onSlashClose() {
  // 用户继续编辑会自然退出触发态;关闭事件本身只清提示
  slashError.value = null
}

function clearCurrentPaneView() {
  clearStreamingTurns()
  hideHistory.value = true
  showHelpCard.value = false
}

function handleNewSession() {
  // /new:在右侧开新 pane(无 sessionId),首次发消息时由 CLI 创建 sid
  if (props.paneId) {
    splitPane(props.paneId, null)
  } else {
    // 非分屏模式:清当前选中,等价于"开新会话"入口
    selectSession(null)
  }
}

function handleChangeDirectory(arg: string) {
  // 严格匹配 display_path(已解码的项目路径)
  const target = projects.value.find(p => p.display_path === arg)
  if (!target) {
    slashError.value = '路径未发现'
    return
  }
  if (target.sessions.length === 0) {
    slashError.value = '该项目暂无会话'
    return
  }
  selectSession(target.sessions[0].id)
}

function handleNativeCommand(cmd: SlashCommand) {
  switch (cmd.name) {
    case 'help':
      showHelpCard.value = true
      scrollToBottom(true)
      break
    case 'clear':
      clearCurrentPaneView()
      break
    case 'new':
      handleNewSession()
      break
    case 'cd': {
      const arg = inputText.value.trim().replace(/^\/cd\s+/, '')
      handleChangeDirectory(arg)
      break
    }
  }
}

function handleModelSwitch(modelName: string) {
  setModel(modelName)
}

async function handleSend() {
  const text = inputText.value.trim()
  if (!text || !currentSession.value) return
  const cs = currentSession.value
  if (!cs.summary.cwd) return

  const parsed = parseCommand(text)

  // /model invalid 等:不清空输入,显示提示
  if (parsed.kind === 'invalid') {
    slashError.value = parsed.reason
    return
  }
  slashError.value = null

  // native 命令(/help /clear /new /cd):前端处理,不调 CLI
  if (parsed.kind === 'native') {
    handleNativeCommand(parsed.cmd)
    inputText.value = ''
    if (textareaRef.value) textareaRef.value.style.height = 'auto'
    return
  }

  // pass 命令(目前仅 /model):持久化设置,不发消息
  if (parsed.kind === 'pass' && parsed.cmd.name === 'model') {
    handleModelSwitch(parsed.arg)
    inputText.value = ''
    if (textareaRef.value) textareaRef.value.style.height = 'auto'
    return
  }

  // unknown / 普通文本:走原始流式发送
  inputText.value = ''
  if (textareaRef.value) textareaRef.value.style.height = 'auto'
  scrollToBottom(true)
  await sendMessage(cs.summary.id, cs.summary.cwd, text, {
    model: settings.value.modelId ?? undefined,
    effort: settings.value.effort,
  })
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}

// 顶栏事件转发
function onSplitRight() {
  if (props.paneId) splitPane(props.paneId, null)
}
function onClose() {
  if (props.paneId) closePane(props.paneId)
}

/** 用户是否在底部附近(60px 阈值) */
function isNearBottom(): boolean {
  const el = scrollContainer.value
  if (!el) return true
  return el.scrollHeight - el.scrollTop - el.clientHeight < 60
}

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

watch(records, () => scrollToBottom(true))

watch(streaming, async (val, oldVal) => {
  if (!val && oldVal) {
    // 流式结束 → reload jsonl(下次切回会话/重启时要从持久化记录读);
    // 但 claude CLI 可能还在 flush,首次 reload 可能拿不到新消息,延迟重试一次
    const beforeLen = records.value.length
    await reloadRecords()
    if (records.value.length === beforeLen) {
      await new Promise(r => setTimeout(r, 400))
      await reloadRecords()
    }
    // 不清 streamingTurns:让它持续渲染本次回复,避免"历史区/流式区"渲染切换造成闪烁。
    // messages computed 已经按 messageId 过滤掉重复,不会双显示。
    // streamingTurns 在切会话 / 新一轮 sendMessage 时自然清空。
  }
})

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
    <!-- 会话顶栏(FR-006) -->
    <SessionTopBar
      :title="displayTitle(currentSession.summary)"
      :session-id="currentSession.summary.id"
      :short-id-value="shortId(currentSession.summary.id)"
      :project-id="currentSession.projectId"
      :cwd="currentSession.summary.cwd"
      :git-branch="currentSession.summary.git_branch"
      :model-string="lastAssistantModel ?? currentSession.summary.model"
      :used-context-tokens="lastAssistantContextSize"
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

    <!-- 加载态 -->
    <div v-if="loading" class="flex-1 flex items-center justify-center">
      <p class="text-default4 text-sm">加载对话中...</p>
    </div>

    <!-- 错误态 -->
    <div v-else-if="error" class="flex-1 flex items-center justify-center">
      <p class="text-red-400 text-sm">{{ error }}</p>
    </div>

    <!-- 无记录 -->
    <div v-else-if="messages.length === 0 && !streaming && !showHelpCard" class="flex-1 flex items-center justify-center">
      <p class="text-default4 text-sm">无对话记录</p>
    </div>

    <!-- 对话消息流 -->
    <div v-else ref="scrollContainer" class="flex-1 overflow-y-auto min-h-0 px-4 py-3 space-y-4 overscroll-contain">
      <template v-if="!hideHistory">
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
      </template>

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
          <TransitionGroup name="block-fade" tag="div" appear>
            <MessageBlock
              v-for="(block, i) in turn.content"
              :key="`${turn.messageId}-${i}`"
              :block="block"
            />
          </TransitionGroup>
        </div>
      </div>

      <div v-if="streaming && streamingTurns.length === 0" class="flex gap-3">
        <div class="w-0.5 shrink-0 rounded-full bg-purple-500/60" />
        <div class="text-xs text-default4">思考中...</div>
      </div>

      <div v-if="streamError" class="px-3 py-2 rounded-md bg-red-500/10 text-red-400 text-xs">
        {{ streamError }}
      </div>

      <!-- /help 本地帮助卡片 -->
      <SlashHelpCard v-if="showHelpCard" :commands="SLASH_COMMANDS" />
    </div>

    <!-- 权限请求卡片(FR-003,固定在输入栏上方) -->
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

    <!-- 输入栏 + 斜杠命令面板 -->
    <div v-if="currentSession.summary.cwd" class="px-4 py-3 border-t border-divider shrink-0 relative">
      <div v-if="slashError" class="mb-1 text-xs text-red-400">
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
        <textarea
          ref="textareaRef"
          v-model="inputText"
          :disabled="streaming"
          placeholder="输入消息… (Shift+Enter 换行,/ 触发命令补全)"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-input border border-divider
                 text-default placeholder-default4 resize-none
                 focus:outline-none focus:border-blue-500/50 transition-colors
                 disabled:opacity-50"
          @keydown="onInputKeydown"
          @input="onInputChange"
          @keyup="syncCursor"
          @click="syncCursor"
          @select="syncCursor"
        />
        <button
          v-if="streaming"
          class="px-3 py-2 text-xs rounded-md bg-red-500/15 text-red-400 hover:bg-red-500/25 transition-colors shrink-0"
          @click="onStopStreaming"
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
