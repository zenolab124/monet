<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { createSessionDetail } from '@/composables/useSessionDetail'
import {
  useStreaming,
  useSessionStream,
  streamingTick,
  finishedDirty,
} from '@/composables/useStreaming'
import { useSessionSettings } from '@/composables/useSessionSettings'
import { useWorkbench } from '@/composables/useWorkbench'
import { useNotifications } from '@/composables/useNotifications'
import {
  SLASH_COMMANDS,
  shouldTriggerPanel,
  parseCommand,
  type SlashCommand,
} from '@/composables/useSlashCommands'
import { shortId, shortModel } from '@/types'
import type { SessionRecord, SessionSummary, ContentBlock } from '@/types'
import MessageBlock from './MessageBlock.vue'
import SystemEventRow from './SystemEventRow.vue'
import SessionTopBar from './topbar/SessionTopBar.vue'
import SlashCommandPanel from './SlashCommandPanel.vue'
import SlashHelpCard from './SlashHelpCard.vue'
import PermissionCard from './PermissionCard.vue'
import {
  usePermissionRequests,
  currentForSession,
} from '@/composables/usePermissionRequests'

/**
 * 会话详情。两种宿主形态(v2.1.0 FR-004/009,档案馆分屏已下线):
 * - mode='archive'(默认):档案馆只读——无输入区/权限交互,底部常驻只读条;流式中会话可只读跟看
 * - mode='workbench':工作台右区列——完整交互(输入/斜杠/权限卡)
 */
const props = defineProps<{
  /** 直接指定会话(工作台列);优先于全局选中 */
  sessionId?: string | null
  mode?: 'archive' | 'workbench'
}>()

/** 是否可交互(输入/权限决策只存在于工作台,FR-009 档案馆移除渲染而非隐藏) */
const interactive = computed(() => props.mode === 'workbench')

const { projects, loadProjects } = useProjects()
const { selectedSessionId, selectSession } = useSessions()
const { findSession, removeSession, draftCwd } = useWorkbench()
const { goToSession } = useNotifications()

// 每个实例独立的 detail 数据
const detail = createSessionDetail()
const { records, loading, error, loadRecords, reloadRecords, clearRecords } = detail

const { sendMessage, stopStreaming, clearStreamingTurns } = useStreaming()

const inputText = ref('')
const scrollContainer = ref<HTMLElement>()
const textareaRef = ref<HTMLTextAreaElement>()

// --- 会话 ID 来源 ---

const effectiveSessionId = computed(() => {
  if (props.sessionId !== undefined) return props.sessionId
  return selectedSessionId.value
})

// per-session 流式状态(v2.1.0:多会话并行,各列独立)
const stream = useSessionStream(effectiveSessionId)

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

// 会话切换时复位 /clear 与 /help 的视图标志,滚动恢复跟随
// (流式区已按会话隔离,无需也不应清流式数据——切回时继续展示)
watch(effectiveSessionId, () => {
  hideHistory.value = false
  showHelpCard.value = false
  followStreaming.value = true
})

// --- 权限请求(仅工作台列交互;档案馆只读不渲染) ---
const permissionRequest = currentForSession(effectiveSessionId)
const { respondRequest, denyAllForSession } = usePermissionRequests()

async function onPermissionDecide(decision: 'allow_once' | 'allow_session' | 'deny') {
  const req = permissionRequest.value
  if (req) await respondRequest(req.requestId, decision)
}

async function onStopStreaming() {
  const sid = effectiveSessionId.value
  if (!sid) return
  await denyAllForSession(sid)
  await stopStreaming(sid)
}

// --- 会话级设置(模型 / 努力等级) ---
const { settings, setModel, setEffort } = useSessionSettings(effectiveSessionId)

function onModelChange(modelId: string) {
  setModel(modelId)
}

function onEffortChange(effort: 'low' | 'medium' | 'high' | 'xhigh' | 'max') {
  setEffort(effort)
}

// --- 会话数据 ---

function onDeleted() {
  const sid = effectiveSessionId.value
  // 在工作台中的会话被删除:一并移出工作台(FR-009)
  if (sid && findSession(sid)) {
    removeSession(sid)
  }
  if (!props.sessionId) {
    selectSession(null)
  }
  clearRecords()
  loadProjects()
}

/** 草稿会话(应用内新建未落盘)的合成 summary:首条消息落盘后自动让位真实数据 */
function draftSummary(id: string, cwd: string): SessionSummary {
  return {
    id,
    title: '新会话',
    first_user_message: null,
    model: null,
    git_branch: null,
    cwd,
    version: null,
    timestamp: null,
    last_modified: Math.floor(Date.now() / 1000),
    total_tokens: {
      input_tokens: 0,
      output_tokens: 0,
      cache_creation_input_tokens: 0,
      cache_read_input_tokens: 0,
    },
    file_size: 0,
    message_count: 0,
  }
}

const currentSession = computed<{ summary: SessionSummary; projectId: string } | null>(() => {
  const sid = effectiveSessionId.value
  if (!sid) return null
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === sid)
    if (s) return { summary: s, projectId: p.id }
  }
  // 工作台草稿:磁盘尚无 jsonl,合成最小 summary;projectId 按目录编码规则(/ → -)推导
  const cwd = draftCwd(sid)
  if (cwd) return { summary: draftSummary(sid, cwd), projectId: cwd.replace(/\//g, '-') }
  return null
})

// --- 只读条(FR-009,仅档案馆) ---

const workbenchHome = computed(() => {
  const sid = effectiveSessionId.value
  if (!sid) return null
  return findSession(sid)?.tab ?? null
})

function onOpenInWorkbench() {
  const sid = effectiveSessionId.value
  if (sid) goToSession(sid)
}

/**
 * 最近一条「真实」assistant 记录(用于推断模型与上下文占用)。
 * 跳过 model 为 <synthetic> 的合成消息(CLI 本地生成的 API Error 占位等)——
 * 它们不代表会话用的模型,且 usage 全 0 会把上下文进度打回零。
 */
const lastAssistantRecord = computed(() => {
  for (let i = records.value.length - 1; i >= 0; i--) {
    const r = records.value[i]
    if (r.type === 'assistant' && r.message && r.message.model !== '<synthetic>') return r
  }
  return null
})

/** 最近 assistant 跑过的真实 model 字符串(含 [1m] 后缀如有);为空时 fallback 到 summary.model */
const lastAssistantModel = computed<string | null>(() => {
  return lastAssistantRecord.value?.message?.model ?? null
})

/**
 * 顶栏展示用 model 字符串。summary.model 兜底也可能是 <synthetic>
 * (整个会话只有合成记录时),过滤防止它被当自定义模型展示甚至被选用。
 */
const displayModelString = computed<string | null>(() => {
  const m = lastAssistantModel.value ?? currentSession.value?.summary.model ?? null
  return m === '<synthetic>' ? null : m
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
  new Set(stream.value.streamingTurns.map(t => t.messageId)),
)

/** 进入消息流的 system 子类型（其余 system 记录为噪音，不渲染） */
const VISIBLE_SYSTEM_SUBTYPES = new Set(['api_error', 'compact_boundary'])

const messages = computed(() => {
  const visible = records.value.filter(
    (r): r is Extract<SessionRecord, { type: 'user' | 'assistant' | 'system' }> => {
      if (r.type === 'assistant') {
        // 当前流式区还在显示这条消息时,历史区跳过(避免双显示)
        const msgId = r.message?.id
        if (msgId && streamingMessageIds.value.has(msgId)) return false
        return true
      }
      if (r.type === 'system') {
        return !!r.subtype && VISIBLE_SYSTEM_SUBTYPES.has(r.subtype)
      }
      if (r.type !== 'user') return false
      const content = r.message?.content
      if (!content || typeof content === 'string') return true
      return content.some((b: ContentBlock) => b.type !== 'tool_result')
    },
  )
  // 连续 api_error（同一请求的多次重试）折叠为最后一条,末条 retryAttempt 自带累计次数
  return visible.filter((r, i) => {
    if (r.type !== 'system' || r.subtype !== 'api_error') return true
    const next = visible[i + 1]
    return !(next?.type === 'system' && next.subtype === 'api_error')
  })
})

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
  const sid = effectiveSessionId.value
  if (sid) clearStreamingTurns(sid)
  hideHistory.value = true
  showHelpCard.value = false
}

function handleNewSession() {
  // /new:工作台列绑定固定会话,引导用左列入口;档案馆回到空选择
  if (props.mode === 'workbench') {
    slashError.value = '工作台中请使用左列「＋」新建'
    return
  }
  selectSession(null)
}

function handleChangeDirectory(arg: string) {
  if (props.mode === 'workbench') {
    slashError.value = '工作台列已绑定会话,请从档案馆打开其他会话'
    return
  }
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
  followStreaming.value = true
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

/** 用户是否在底部附近(60px 阈值) */
function isNearBottom(): boolean {
  const el = scrollContainer.value
  if (!el) return true
  return el.scrollHeight - el.scrollTop - el.clientHeight < 60
}

/**
 * 滚动跟随状态:用户向上滚动(wheel 方向)即脱离——不能只靠 60px 阈值判定,
 * 触控板起步每帧只移动几 px,逃不出阈值区间就会被每帧的跟随滚动拽回(锁死)。
 * 滚回底部自动恢复;发消息/切会话重置为跟随。
 */
const followStreaming = ref(true)

function onScrollWheel(e: WheelEvent) {
  if (e.deltaY < 0) followStreaming.value = false
}

function onScroll() {
  if (!followStreaming.value && isNearBottom()) followStreaming.value = true
}

function resumeFollow() {
  followStreaming.value = true
  scrollToBottom(true)
}

function scrollToBottom(force = false) {
  if (!force && !isNearBottom()) return
  nextTick(() => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const el = scrollContainer.value
        if (!el) return
        // 全程 instant:smooth 在字符级流式高频调用下会被浏览器取消重启动画,
        // 反而出现"弹跳"。逐帧 instant 移动小距离,60fps 下肉眼是连续平滑的。
        el.scrollTo({ top: el.scrollHeight, behavior: 'instant' })
      })
    })
  })
}

// 记录变化(加载/流后落账)只在跟随态回底:用户上滚阅读时不打扰
watch(records, () => {
  if (followStreaming.value) scrollToBottom(true)
})

// 本会话流式结束 → 等 jsonl 落账后 reload(只在本实例正在展示该会话时发生)
watch(() => stream.value.streaming, async (val, oldVal) => {
  if (!val && oldVal) {
    const cs = currentSession.value
    if (!cs) return
    const sid = cs.summary.id
    // 等 jsonl flush 稳定(经验值 300ms),避免触发"首次 reload 没拿到 → 重试一次"的双重排
    await new Promise(r => setTimeout(r, 300))
    let newRecords: SessionRecord[] | null = null
    try {
      newRecords = await invoke<SessionRecord[]>('get_session_records', {
        projectId: cs.projectId,
        sessionId: sid,
      })
    } catch {
      // ignore:走兜底分支
    }
    if (!newRecords || newRecords.length === records.value.length) {
      // 没拿到 / jsonl 还没写完,再等一次
      await new Promise(r => setTimeout(r, 400))
      try {
        newRecords = await invoke<SessionRecord[]>('get_session_records', {
          projectId: cs.projectId,
          sessionId: sid,
        })
      } catch {
        // ignore
      }
    }
    // 关键:reload 后只更新 records + 清 pendingUserMessage,
    // **不**清 streamingTurns——保留流式累积的组件实例,markdown-it+shiki 不重渲染。
    // streamingMessageIds 继续屏蔽历史区中相同 messageId 的 assistant record(无双显示)。
    if (effectiveSessionId.value !== sid) return
    if (newRecords) records.value = newRecords
    stream.value.pendingUserMessage = null
    finishedDirty.delete(sid)
  }
})

// 滚动跟随:watch streamingTick(打字机每帧递增,统一覆盖各种 mutation),
// 仅本会话流式中且用户未脱离跟随时才滚
watch(streamingTick, () => {
  if (stream.value.streaming && followStreaming.value) scrollToBottom()
})

watch(
  () => currentSession.value,
  async (cs) => {
    if (cs) {
      // 后台结束的流式(本实例未挂载期间)留下脏标记 → 强制刷新拿落账记录
      const force = finishedDirty.has(cs.summary.id)
      if (force) finishedDirty.delete(cs.summary.id)
      await loadRecords(cs.projectId, cs.summary.id, force)
      // 落账记录已到手:残留的流式区(turns/pendingUserMessage)让位,
      // 否则历史区 user record 与 pendingUserMessage 双显、
      // 残留 turn 经 streamingMessageIds 挡住历史区同 id 的完整记录
      if (force && !stream.value.streaming && effectiveSessionId.value === cs.summary.id) {
        clearStreamingTurns(cs.summary.id)
      }
    } else {
      clearRecords()
    }
  },
  { immediate: true },
)

/** 顶栏手动刷新:磁盘记录为权威,流式已结束则连流式区残留一并重置 */
async function onReload() {
  await reloadRecords()
  const sid = effectiveSessionId.value
  if (sid && !stream.value.streaming) {
    clearStreamingTurns(sid)
  }
}
</script>

<template>
  <!-- 空态 -->
  <div v-if="!currentSession" class="h-full flex items-center justify-center">
    <p class="text-muted-foreground text-sm">{{ mode === 'workbench' ? '会话不存在或已删除' : '从左侧选择会话' }}</p>
  </div>

  <div v-else class="h-full flex flex-col">
    <!-- 会话顶栏(单行极简:标题由列头/列表承担,不重复显示) -->
    <SessionTopBar
      :session-id="currentSession.summary.id"
      :short-id-value="shortId(currentSession.summary.id)"
      :project-id="currentSession.projectId"
      :cwd="currentSession.summary.cwd"
      :git-branch="currentSession.summary.git_branch"
      :model-string="displayModelString"
      :used-context-tokens="lastAssistantContextSize"
      :last-modified="currentSession.summary.last_modified"
      :selected-model-id="settings.modelId"
      :selected-effort="settings.effort"
      @model-change="onModelChange"
      @effort-change="onEffortChange"
      @reload="onReload"
      @deleted="onDeleted"
    />

    <!-- 加载态 -->
    <div v-if="loading" class="flex-1 flex items-center justify-center">
      <p class="text-muted-foreground text-sm">加载对话中...</p>
    </div>

    <!-- 错误态 -->
    <div v-else-if="error" class="flex-1 flex items-center justify-center">
      <p class="text-destructive text-sm">{{ error }}</p>
    </div>

    <!-- 无记录(草稿会话给引导文案) -->
    <div v-else-if="messages.length === 0 && !stream.streaming && !showHelpCard" class="flex-1 flex items-center justify-center">
      <p class="text-muted-foreground text-sm">
        {{ effectiveSessionId && draftCwd(effectiveSessionId) ? '新会话 — 输入第一条消息开始对话' : '无对话记录' }}
      </p>
    </div>

    <!-- 对话消息流 -->
    <div
      v-else
      ref="scrollContainer"
      class="flex-1 overflow-y-auto min-h-0 px-4 py-3 space-y-4 overscroll-contain relative"
      @wheel.passive="onScrollWheel"
      @scroll.passive="onScroll"
    >
      <template v-if="!hideHistory">
        <template v-for="(msg, i) in messages" :key="msg.uuid || `msg-${i}`">
          <!-- system 事件行：无气泡形态,独立渲染 -->
          <SystemEventRow v-if="msg.type === 'system'" :record="msg" />
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
                <span v-if="msg.type === 'assistant' && msg.message?.model" class="text-muted-foreground font-normal">
                  ({{ shortModel(msg.message.model) }})
                </span>
              </div>
              <!-- 历史区用普通 div 包裹:与流式区 TransitionGroup(tag="div") 的 DOM
                   层级一致(切换时父容器无 layout 重排),但绝不播动画——发送瞬间上一轮
                   内容从流式区转移进历史区,是 v-for 新成员,若包 TransitionGroup 会把
                   旧内容重新淡入一遍(150ms 闪烁)。动画只属于流式区的真实新块。 -->
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
      </template>

      <div v-if="stream.pendingUserMessage" class="flex gap-3 msg-block">
        <div class="w-0.5 shrink-0 rounded-full bg-primary/60" />
        <div class="min-w-0 flex-1">
          <div class="text-xs font-medium mb-1 text-primary">你</div>
          <div class="whitespace-pre-wrap break-words text-sm">{{ stream.pendingUserMessage }}</div>
        </div>
      </div>

      <div v-for="turn in stream.streamingTurns" :key="turn.messageId" class="flex gap-3 msg-block">
        <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
        <div class="min-w-0 flex-1">
          <div class="text-xs font-medium mb-1 text-claude">Claude</div>
          <!-- key 命名跟历史区对齐(纯索引),减少结束切换时 Vue diff 的额外 keyed-list 比对 -->
          <TransitionGroup name="block-fade" tag="div" appear>
            <MessageBlock
              v-for="(block, i) in turn.content"
              :key="i"
              :block="block"
            />
          </TransitionGroup>
        </div>
      </div>

      <div v-if="stream.streaming && stream.streamingTurns.length === 0" class="flex gap-3">
        <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
        <div class="text-xs text-muted-foreground">思考中...</div>
      </div>

      <div v-if="stream.streamError" class="px-3 py-2 rounded-md bg-destructive/10 text-destructive text-xs">
        {{ stream.streamError }}
      </div>

      <!-- /help 本地帮助卡片 -->
      <SlashHelpCard v-if="showHelpCard" :commands="SLASH_COMMANDS" />

      <!-- 脱离跟随提示:流式中用户上滚阅读时,贴滚动视口底部 -->
      <div
        v-if="stream.streaming && !followStreaming"
        class="sticky bottom-0 flex justify-center pointer-events-none"
      >
        <button
          class="pointer-events-auto px-3 py-1 text-xs rounded-full bg-popover border border-border
                 shadow-paper text-muted-foreground hover:text-foreground transition-colors
                 flex items-center gap-1"
          @click="resumeFollow"
        >
          <span class="i-carbon-arrow-down w-3 h-3" />
          回到底部
        </button>
      </div>
    </div>

    <!-- 工作台列:权限请求卡片(固定在输入栏上方) -->
    <div
      v-if="interactive && permissionRequest"
      class="px-4 pb-2 shrink-0 flex justify-center"
    >
      <PermissionCard
        :key="permissionRequest.requestId"
        :request="permissionRequest"
        @decide="onPermissionDecide"
      />
    </div>

    <!-- 工作台列:输入栏 + 斜杠命令面板(档案馆只读化:整块不渲染,事件绑定随之移除) -->
    <div v-if="interactive && currentSession.summary.cwd" class="px-4 py-3 border-t border-border shrink-0 relative">
      <div v-if="slashError" class="mb-1 text-xs text-destructive">
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
          :disabled="stream.streaming"
          placeholder="输入消息… (Shift+Enter 换行,/ 触发命令补全)"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-popover border border-border
                 text-foreground placeholder-muted-foreground resize-none
                 focus:outline-none focus:border-ring transition-colors
                 disabled:opacity-50"
          @keydown="onInputKeydown"
          @input="onInputChange"
          @keyup="syncCursor"
          @click="syncCursor"
          @select="syncCursor"
        />
        <button
          v-if="stream.streaming"
          class="px-3 py-2 text-xs rounded-md bg-accent text-accent-foreground hover:shadow-paper transition-shadow shrink-0"
          @click="onStopStreaming"
        >
          停止
        </button>
        <button
          v-else
          :disabled="!inputText.trim()"
          class="px-3 py-2 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow shrink-0
                 disabled:opacity-30 disabled:cursor-not-allowed"
          @click="handleSend"
        >
          发送
        </button>
      </div>
    </div>

    <!-- 档案馆:常驻只读条(FR-009) -->
    <div
      v-if="!interactive"
      class="px-4 py-2 border-t border-border shrink-0 flex items-center gap-2 text-xs text-muted-foreground"
    >
      <span class="i-carbon-document w-3.5 h-3.5 shrink-0" />
      <span v-if="workbenchHome" class="truncate">正在「{{ workbenchHome.name }}」工作台运行</span>
      <span v-else class="truncate">只读预览</span>
      <button
        class="ml-auto shrink-0 px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow"
        @click="onOpenInWorkbench"
      >
        {{ workbenchHome ? '前往' : '在工作台打开' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.msg-block {
  contain: layout style;
}
</style>
