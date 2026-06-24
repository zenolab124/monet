<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted, provide } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { createSessionDetail } from '@/composables/useSessionDetail'
import {
  useStreaming,
  useSessionStream,
  streamingTick,
  finishedDirty,
} from '@/composables/useStreaming'
import { useSessionSettings, ADVISOR_MAIN_MODEL, type ChannelMark } from '@/composables/useSessionSettings'
import { useAppDefaults } from '@/composables/useAppDefaults'
import {
  refreshChannels,
  resolveChannel,
  channelDisplayName,
  OFFICIAL_CHANNEL_ID,
} from '@/composables/useChannels'
import { useWorkbench } from '@/composables/useWorkbench'
import { useNotifications } from '@/composables/useNotifications'
import {
  SLASH_COMMANDS,
  shouldTriggerPanel,
  parseCommand,
  type SlashCommand,
} from '@/composables/useSlashCommands'
import { useSessionMeta } from '@/composables/useSessionMeta'
import { shortId, shortModel } from '@/types'
import { filterConsumedResults, type ToolResultData } from '@/utils/toolPair'
import type { SessionRecord, SessionSummary, ContentBlock } from '@/types'
import MessageBlock from './MessageBlock.vue'
import SystemEventRow from './SystemEventRow.vue'
import SessionTopBar from './topbar/SessionTopBar.vue'
import SlashCommandPanel from './SlashCommandPanel.vue'
import SlashHelpCard from './SlashHelpCard.vue'
import PermissionCard from './PermissionCard.vue'
import QuestionCard from './QuestionCard.vue'
import PlanApprovalCard from './PlanApprovalCard.vue'
import MsgClamp from './MsgClamp.vue'
import { useImageInput } from '@/composables/useImageInput'
import { useHtmlVisual, HTML_VISUAL_PROMPT } from '@/features'
import SessionBanner from './SessionBanner.vue'
import {
  usePermissionRequests,
  currentForSession,
  type RespondExtra,
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
  /** 赛马模式:隐藏列内输入框(共享输入在底部) */
  hideInput?: boolean
}>()

const { t } = useI18n()

/** 是否可交互(输入/权限决策只存在于工作台,FR-009 档案馆移除渲染而非隐藏) */
const interactive = computed(() => props.mode === 'workbench')

const { projects, loadProjects } = useProjects()
const { selectedSessionId, selectSession } = useSessions()
const { findSession, removeSession, draftCwd } = useWorkbench()
const { goToSession } = useNotifications()

// 每个实例独立的 detail 数据
const detail = createSessionDetail()
const { records, loading, error, loadRecords, reloadRecords, clearRecords } = detail

const { sendMessage, stopStreaming, clearStreamingTurns, removePendingQueueItem, consumePendingQueue } = useStreaming()

const { enabled: htmlVisualEnabled } = useHtmlVisual()
const featureBannerShown = ref(false)
const bannerResumed = ref(false)
const bannerCwd = ref('')
interface HookEvent {
  subtype: 'hook_started' | 'hook_response'
  hook_name: string
  hook_event: string
  output?: string
  exit_code?: number
}
const bannerHookEvents = ref<HookEvent[]>([])

const inputText = ref('')
const scrollContainer = ref<HTMLElement>()
const textareaRef = ref<HTMLTextAreaElement>()

const imageInput = useImageInput({ pasteTarget: textareaRef })
onMounted(() => imageInput.attach())

interface SessionConnectedPayload {
  session_id: string
  resumed: boolean
  cwd: string
}

let unlistenConnected: (() => void) | null = null
let unlistenHook: (() => void) | null = null

listen<SessionConnectedPayload>('session-connected', (e) => {
  const p = e.payload
  if (p.session_id === effectiveSessionId.value) {
    bannerResumed.value = p.resumed
    bannerCwd.value = p.cwd
    bannerHookEvents.value = []
    featureBannerShown.value = true
    nextTick(() => scrollToBottom(true))
  }
}).then(fn => { unlistenConnected = fn })

listen<HookEvent & { session_id: string }>('session-hook', (e) => {
  const p = e.payload
  if (p.session_id === effectiveSessionId.value) {
    bannerHookEvents.value.push(p)
  }
}).then(fn => { unlistenHook = fn })

onUnmounted(() => {
  unlistenConnected?.()
  unlistenHook?.()
})

// --- 会话 ID 来源 ---

const effectiveSessionId = computed(() => {
  if (props.sessionId !== undefined) return props.sessionId
  return selectedSessionId.value
})

// per-session 流式状态(v2.1.0:多会话并行,各列独立)
const stream = useSessionStream(effectiveSessionId)

// --- tool_result 全局查找表(跨消息配对:tool_use 在 assistant、tool_result 在 user) ---
const toolResultMap = computed(() => {
  const map = new Map<string, ToolResultData>()
  for (const r of records.value) {
    if (r.type !== 'user' || !r.message) continue
    const content = r.message.content
    if (typeof content === 'string') continue
    for (const b of content) {
      if (b.type === 'tool_result') {
        const tr = b as Extract<ContentBlock, { type: 'tool_result' }>
        map.set(tr.tool_use_id, { content: tr.content, is_error: tr.is_error })
      }
    }
  }
  for (const turn of stream.value.streamingTurns) {
    for (const b of turn.content) {
      if (b.type === 'tool_result') {
        const tr = b as Extract<ContentBlock, { type: 'tool_result' }>
        map.set(tr.tool_use_id, { content: tr.content, is_error: tr.is_error })
      }
    }
  }
  return map
})
provide('toolResultMap', toolResultMap)

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
  featureBannerShown.value = false
  bannerResumed.value = false
  bannerCwd.value = ''
  bannerHookEvents.value = []
  lastScrollTop = 0
})

// --- 权限请求(仅工作台列交互;档案馆只读不渲染) ---
const permissionRequest = currentForSession(effectiveSessionId)
const { respondRequest, denyAllForSession } = usePermissionRequests()

/** 按工具分发卡片:提问/计划批准走专用交互卡,其余走通用权限卡 */
const requestCard = computed(() => {
  switch (permissionRequest.value?.toolName) {
    case 'AskUserQuestion': return QuestionCard
    case 'ExitPlanMode': return PlanApprovalCard
    default: return PermissionCard
  }
})

async function onPermissionDecide(
  decision: 'allow_once' | 'allow_session' | 'deny',
  extra?: RespondExtra,
) {
  const req = permissionRequest.value
  if (req) await respondRequest(req.requestId, decision, extra)
}

async function onStopStreaming() {
  const sid = effectiveSessionId.value
  if (!sid) return
  await denyAllForSession(sid)
  await stopStreaming(sid)
}

// --- 会话级设置(模型 / 努力等级 / 渠道) ---
const { settings, setModel, setEffort, setChannel, setAdvisor, setPermissionMode: persistPermissionMode } = useSessionSettings(effectiveSessionId)
const { appDefaults } = useAppDefaults()

function onModelChange(modelId: string) {
  setModel(modelId)
}

function onEffortChange(effort: 'low' | 'medium' | 'high' | 'xhigh' | 'max' | 'ultracode' | null) {
  setEffort(effort)
}

// --- 渠道(per-session 选择 + 切换横线记账) ---

// 渠道名解析(badge/横线)需要清单:实例创建时拉一次,下拉每次打开还会各自重读
refreshChannels()

/** 解析后的最终注入渠道 id(null = 官方/零注入):发送与终端恢复共用 */
const resolvedChannelId = computed(() => resolveChannel(settings.value.channelId))

function onChannelChange(channelId: string | null) {
  const list = messages.value
  const last = list.length > 0 ? list[list.length - 1] : null
  setChannel(channelId, last?.uuid ?? null)
}

function onAdvisorChange(advisor: boolean) {
  setAdvisor(advisor)
}

function onPermissionModeChange(mode: import('@/composables/useSessionSettings').PermissionMode) {
  persistPermissionMode(mode)
}

/** 当前消息流里出现的 uuid 集合:判断 mark 锚点是否还在视图内 */
const messageUuidSet = computed(() => {
  const set = new Set<string>()
  for (const m of messages.value) if (m.uuid) set.add(m.uuid)
  return set
})

/**
 * 切换横线按锚点消息 uuid 分组(null = 会话起点的切换)。
 * 锚点消息已不在视图内(被 api_error 折叠吞掉 / 流式中切换尚未落盘 / /clear 隐藏历史)的 mark
 * 不进此表,改由 unanchoredChannelMarks 在消息流末尾兜底渲染——绝不静默消失。
 */
const channelMarksByUuid = computed(() => {
  const map = new Map<string | null, ChannelMark[]>()
  for (const m of settings.value.channelMarks) {
    if (m.afterUuid !== null && !messageUuidSet.value.has(m.afterUuid)) continue
    const group = map.get(m.afterUuid)
    if (group) group.push(m)
    else map.set(m.afterUuid, [m])
  }
  return map
})

/** 锚点已失效的切换横线:统一在消息流末尾按顺序兜底渲染 */
const unanchoredChannelMarks = computed(() =>
  settings.value.channelMarks.filter(
    m => m.afterUuid !== null && !messageUuidSet.value.has(m.afterUuid),
  ),
)

function channelMarkLabel(m: ChannelMark): string {
  if (m.channelId === null) return t('session.channelSwitchedDefault')
  if (m.channelId === OFFICIAL_CHANNEL_ID) return t('session.channelSwitchedOfficial')
  return t('session.channelSwitched', { name: channelDisplayName(m.channelId) })
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
    title: t('session.newSessionTitle'),
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
    context_window: null,
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

const { getMeta, updateMeta } = useSessionMeta()
const summaryGenerating = ref(false)

const currentSummary = computed(() => {
  const sid = effectiveSessionId.value
  return sid ? getMeta(sid)?.summary : undefined
})

async function onGenerateSummary() {
  const cs = currentSession.value
  if (!cs || summaryGenerating.value) return
  summaryGenerating.value = true
  try {
    const summary = await invoke<string>('generate_summary', { projectId: cs.projectId, sessionId: cs.summary.id })
    await updateMeta(cs.summary.id, { summary } as any)
  } catch (e) {
    console.warn('[meta] 摘要生成失败:', e)
  } finally {
    summaryGenerating.value = false
  }
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

/** 流式区 pendingUserMessage 对应的 user record uuid（防历史区双显）。
 *  从后向前扫描所有 user record 做文本匹配，跳过纯 tool_result 的中间记录，
 *  找到文本一致的那条返回其 uuid。 */
const pendingUserUuid = computed(() => {
  if (!stream.value.pendingUserMessage || !stream.value.streamingTurns.length) return null
  const pendingText = stream.value.pendingUserMessage
  for (let i = records.value.length - 1; i >= 0; i--) {
    const r = records.value[i]
    if (r.type !== 'user') continue
    const content = r.message?.content
    const text = typeof content === 'string'
      ? content
      : Array.isArray(content)
        ? (content.find((b: ContentBlock) => b.type === 'text') as { text?: string } | undefined)?.text
        : undefined
    if (text === pendingText) return r.uuid
  }
  return null
})

const messages = computed(() => {
  const pUuid = pendingUserUuid.value
  const visible = records.value.filter(
    (r): r is Extract<SessionRecord, { type: 'user' | 'assistant' | 'system' }> => {
      if (r.type === 'assistant') {
        const msgId = r.message?.id
        if (msgId && streamingMessageIds.value.has(msgId)) return false
        return true
      }
      if (r.type === 'system') {
        return !!r.subtype && VISIBLE_SYSTEM_SUBTYPES.has(r.subtype)
      }
      if (r.type !== 'user') return false
      // 流式区 pendingUserMessage 还在显示时,历史区跳过对应的 user record
      if (pUuid && r.uuid === pUuid) return false
      const content = r.message?.content
      if (!content || typeof content === 'string') return true
      return content.some((b: ContentBlock) => b.type !== 'tool_result')
    },
  )
  return visible.filter((r, i) => {
    if (r.type !== 'system' || r.subtype !== 'api_error') return true
    const next = visible[i + 1]
    return !(next?.type === 'system' && next.subtype === 'api_error')
  })
})

type VisibleRecord = Extract<SessionRecord, { type: 'user' | 'assistant' | 'system' }>

interface MsgGroup {
  user: VisibleRecord | null
  responses: VisibleRecord[]
}

const messageGroups = computed(() => {
  const groups: MsgGroup[] = []
  let cur: MsgGroup = { user: null, responses: [] }
  for (const msg of messages.value) {
    if (msg.type === 'user') {
      if (cur.user || cur.responses.length) groups.push(cur)
      cur = { user: msg, responses: [] }
    } else {
      cur.responses.push(msg)
    }
  }
  if (cur.user || cur.responses.length) groups.push(cur)
  return groups
})

/** 解析私有标签,转为特殊渲染块 */
const TAG_RE = /<(system-reminder|ide_opened_file|ide_selection|task-notification|user-prompt-submit-hook|persisted-output|tool_use_error|command-name|command-args|command-message|local-command-caveat|local-command-stdout|loop-pause)[^>]*>([\s\S]*?)<\/\1>/g
const DISCARD_TAGS_RE = /<\/?(?:antml:thinking|antml:function_calls|antml:invoke|antml:parameter)[^>]*>/g
/** 冗余/仅供模型阅读的标签,解析后直接丢弃不渲染 */
const SILENT_TAGS = new Set(['command-message', 'local-command-caveat', 'loop-pause'])

function parsePrivateTags(text: string): ContentBlock[] {
  const results: ContentBlock[] = []
  let lastIndex = 0
  const cleaned = text.replace(DISCARD_TAGS_RE, '')

  for (const match of cleaned.matchAll(TAG_RE)) {
    const before = cleaned.slice(lastIndex, match.index).trim()
    if (before) results.push({ type: 'text', text: before })

    const [, tag, content] = match
    if (!SILENT_TAGS.has(tag)) {
      results.push({ type: tag, text: content.trim() } as any)
    }

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
  const expanded = blocks.flatMap(b => {
    if (b.type !== 'text') return [b]
    const text = (b as any).text as string
    // 系统自动文本：用户中断
    if (/^\[Request interrupted by user/.test(text)) {
      return [{ type: 'system-event', text: text.slice(1, -1) } as any]
    }
    // 系统自动文本：图片尺寸元数据
    if (/^\[Image: (?:original|source:)/.test(text)) {
      return [{ type: 'image-meta', text: text.slice(1, -1) } as any]
    }
    const skillMatch = text.match(/^Base directory for this skill:\s*(\S+)/)
    if (skillMatch) {
      const skillPath = skillMatch[1]
      const skillName = skillPath.split('/').pop() || skillPath
      return [{ type: 'skill_prompt', text: text, name: skillName } as any]
    }
    if (/<(?:system-reminder|ide_opened_file|ide_selection|task-notification|user-prompt-submit-hook|persisted-output|tool_use_error|command-name|command-args|command-message|local-command-caveat|local-command-stdout|loop-pause)/.test(text)) {
      return parsePrivateTags(text)
    }
    return [b]
  })
  // 将 tool_result 内嵌的图片提升到顶层独立渲染
  const lifted: ContentBlock[] = []
  for (const b of expanded) {
    lifted.push(b)
    if (b.type === 'tool_result' && Array.isArray((b as any).content)) {
      for (const sub of (b as any).content as ContentBlock[]) {
        if (sub.type === 'image') lifted.push(sub)
      }
    }
  }
  return lifted
}

const USER_CONTENT_TYPES = new Set(['text', 'image', 'document'])

function isSystemOnlyUser(record: Extract<SessionRecord, { type: 'user' }>): boolean {
  const blocks = contentBlocks(record as any)
  return blocks.length > 0 && blocks.every(b => !USER_CONTENT_TYPES.has(b.type))
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
    slashError.value = t('session.slashNewInWorkbench')
    return
  }
  selectSession(null)
}

function handleChangeDirectory(arg: string) {
  if (props.mode === 'workbench') {
    slashError.value = t('session.slashOpenInWorkbench')
    return
  }
  // 严格匹配 display_path(已解码的项目路径)
  const target = projects.value.find(p => p.display_path === arg)
  if (!target) {
    slashError.value = t('session.slashPathNotFound')
    return
  }
  if (target.sessions.length === 0) {
    slashError.value = t('session.slashNoSessions')
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
  if ((!text && !imageInput.images.value.length) || !currentSession.value) return
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
  featureBannerShown.value = false
  scrollToBottom(true)
  const advisor = settings.value.advisor
  const images = imageInput.images.value.length ? await imageInput.toImageBlocks() : undefined
  imageInput.clearImages()
  await refreshChannels()
  await sendMessage(cs.summary.id, cs.summary.cwd, text, {
    model: advisor ? ADVISOR_MAIN_MODEL : (settings.value.modelId ?? undefined),
    effort: settings.value.effort ?? appDefaults.value.effort,
    channel: resolvedChannelId.value,
    advisor,
    images,
    permissionMode: settings.value.permissionMode,
  })
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}

const followStreaming = ref(true)
let lastScrollTop = 0
let resumedAt = 0
let scrollCoalesced = false
let scrollRafId = 0
let programmaticScroll = false

function onScrollWheel(e: WheelEvent) {
  if (e.deltaY < -3) {
    const el = scrollContainer.value
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 5) return
    followStreaming.value = false
  }
}

function onScroll() {
  if (scrollRafId) return
  scrollRafId = requestAnimationFrame(() => {
    scrollRafId = 0
    const el = scrollContainer.value
    if (!el) return
    if (programmaticScroll) {
      lastScrollTop = el.scrollTop
      programmaticScroll = false
      return
    }
    const delta = el.scrollTop - lastScrollTop
    lastScrollTop = el.scrollTop
    if (performance.now() - resumedAt < 300) return
    if (delta < 0 && el.scrollHeight - el.scrollTop - el.clientHeight > 5) {
      followStreaming.value = false
    } else if (delta > 0 && !followStreaming.value) {
      const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
      const threshold = Math.max(el.clientHeight * 0.5, 400)
      if (distFromBottom < threshold) {
        followStreaming.value = true
        resumedAt = performance.now()
      }
    }
  })
}

function resumeFollow() {
  followStreaming.value = true
  resumedAt = performance.now()
  scrollToBottom(true)
}

function scrollToBottom(force = false) {
  if (!force && !followStreaming.value) return
  if (scrollCoalesced) return
  scrollCoalesced = true
  nextTick(() => {
    requestAnimationFrame(() => {
      scrollCoalesced = false
      const el = scrollContainer.value
      if (!el) return
      programmaticScroll = true
      el.scrollTop = el.scrollHeight
      // 内容刚挂载时 scrollHeight 可能还是 0，延迟重试一次
      if (el.scrollHeight <= el.clientHeight) {
        requestAnimationFrame(() => {
          programmaticScroll = true
          el.scrollTop = el.scrollHeight
        })
      }
    })
  })
}

watch(() => stream.value.pendingUserMessage, (val) => {
  if (val) scrollToBottom(true)
})

watch(records, () => {
  if (followStreaming.value) scrollToBottom(true)
})

// ====== 排除法调试开关（定位闪烁根因后删除）======
// 试法：先 SKIP_RECORDS_RELOAD=true 跑一次，看闪不闪；
//       再换成 false，去 useStreaming 把 SKIP_STREAMING_FALSE 设 true 跑一次。
//       哪个不闪了就是哪个的锅。
const SKIP_RECORDS_RELOAD = false   // true = 跳过 records 更新

watch(() => stream.value.streaming, async (val, oldVal) => {
  if (!val && oldVal) {
    const cs = currentSession.value
    if (!cs) return
    const sid = cs.summary.id
    console.log(`%c ========== [detail] streaming→false, wait 300ms sid=${sid.slice(0, 8)} t=${performance.now().toFixed(0)} ==========`, 'color:#22c55e;font-weight:bold')
    // 立即删除 finishedDirty，防止 meta generation 触发 currentSession watch 时误判为后台落账
    finishedDirty.delete(sid)
    if (SKIP_RECORDS_RELOAD) {
      console.log('%c ========== [detail] SKIP_RECORDS_RELOAD — 跳过 records 更新 ==========', 'color:#ef4444;font-weight:bold')
      return
    }
    await new Promise(r => setTimeout(r, 300))
    let newRecords: SessionRecord[] | null = null
    try {
      newRecords = await invoke<SessionRecord[]>('get_session_records', {
        projectId: cs.projectId,
        sessionId: sid,
      })
    } catch {
      // ignore
    }
    if (!newRecords || newRecords.length === records.value.length) {
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
    if (effectiveSessionId.value !== sid) return
    console.log(`%c ========== [detail] records reload: old=${records.value.length} new=${newRecords?.length ?? 'null'} sid=${sid.slice(0, 8)} t=${performance.now().toFixed(0)} ==========`, 'color:#22c55e;font-weight:bold')
    if (newRecords) records.value = newRecords
    if (cs.summary.cwd) consumePendingQueue(sid, cs.summary.cwd)
  }
})

// 滚动跟随:watch streamingTick(打字机每帧递增,统一覆盖各种 mutation),
// 仅本会话流式中且用户未脱离跟随时才滚
watch(streamingTick, () => {
  if (stream.value.streaming && followStreaming.value) scrollToBottom()
})

// --- 外部运行跟随 ---
//
// 应用关闭后 claude CLI 子进程不随窗口退出(刻意保留:任务继续跑),重开应用后
// stdout 管道已不可重接。改走伪流式:探测到该会话仍有 CLI 进程在跑(命令行含
// session-id)时,周期静默 reload jsonl 落账记录 + 保持滚动跟随,进程退出后做
// 一次收尾 reload。整段追加无打字机,但进度不再需要手动刷新。
// 注意:声明须在下方 immediate watch 之前(其回调在 setup 同步阶段就会执行)。

const externalRunning = ref(false)
let followSessionId: string | null = null
let externalTimer: number | null = null
let probing = false
let externalIdleTicks = 0

/** 静默重载:不动 loading 态/滚动状态,记录数有增长才替换(jsonl 仅追加) */
async function silentReloadRecords() {
  const cs = currentSession.value
  if (!cs) return
  try {
    const fresh = await invoke<SessionRecord[]>('get_session_records', {
      projectId: cs.projectId,
      sessionId: cs.summary.id,
    })
    // 异步窗口内可能已切会话
    if (effectiveSessionId.value !== cs.summary.id) return
    if (fresh.length > records.value.length) records.value = fresh
  } catch {
    // 下一轮探测重试
  }
}

async function probeExternal() {
  if (probing) return
  probing = true
  try {
    const cs = currentSession.value
    // 本地流式由 stream-event 实时驱动,无需外部探测
    if (!cs || stream.value.streaming) {
      externalRunning.value = false
      externalIdleTicks = 0
      stopExternalFollow()
      return
    }
    let running = false
    try {
      running = await invoke<boolean>('check_session_running', { sessionId: cs.summary.id })
    } catch {
      // 探测失败视为未运行
    }
    if (effectiveSessionId.value !== cs.summary.id) return
    if (running) {
      const prevCount = records.value.length
      await silentReloadRecords()
      const changed = records.value.length > prevCount
      if (changed) {
        externalIdleTicks = 0
        if (!externalRunning.value) {
          externalRunning.value = true
          followStreaming.value = true
        }
      } else if (externalRunning.value) {
        externalIdleTicks++
        if (externalIdleTicks >= 4) {
          // 连续 4 轮(~6s)无新输出 → 外部长活进程闲置,解除锁定
          externalRunning.value = false
        }
      } else {
        externalIdleTicks++
        if (externalIdleTicks >= 4) {
          // 从未产出过新记录 → 闲置进程,停止探测
          stopExternalFollow()
        }
      }
    } else if (externalRunning.value) {
      // 进程刚退出:收尾 reload 拿最终落账,结束跟随
      externalRunning.value = false
      externalIdleTicks = 0
      await silentReloadRecords()
      stopExternalFollow()
    } else {
      externalIdleTicks = 0
      stopExternalFollow()
    }
  } finally {
    probing = false
  }
}

function startExternalFollow() {
  stopExternalFollow()
  externalRunning.value = false
  externalIdleTicks = 0
  // 先起定时器再立即探一次:未运行的会话首轮探测即自停,运行中的持续跟随
  externalTimer = window.setInterval(probeExternal, 1500)
  probeExternal()
}

function stopExternalFollow() {
  if (externalTimer !== null) {
    clearInterval(externalTimer)
    externalTimer = null
  }
}

onUnmounted(stopExternalFollow)

let loadedSessionId: string | null = null

watch(
  () => currentSession.value,
  async (cs) => {
    if (cs) {
      const force = finishedDirty.has(cs.summary.id)
      if (force) finishedDirty.delete(cs.summary.id)
      // 同一会话 summary 属性变化(标题/标签等)不重新加载 records,
      // 只有切换会话或 force(后台流式落账)才刷新
      if (cs.summary.id !== loadedSessionId || force) {
        loadedSessionId = cs.summary.id
        await loadRecords(cs.projectId, cs.summary.id, force)
        if (force && !stream.value.streaming && effectiveSessionId.value === cs.summary.id) {
          clearStreamingTurns(cs.summary.id)
        }
        scrollToBottom(true)
      }
      if (cs.summary.id !== followSessionId) {
        followSessionId = cs.summary.id
        startExternalFollow()
      }
    } else {
      loadedSessionId = null
      followSessionId = null
      stopExternalFollow()
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
    <p class="text-muted-foreground text-sm">{{ mode === 'workbench' ? $t('session.notExist') : $t('archive.selectSession') }}</p>
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
      :used-context-tokens="stream.realUsedTokens ?? lastAssistantContextSize"
      :real-context-window="stream.realContextWindow ?? currentSession.summary.context_window ?? null"
      :last-modified="currentSession.summary.last_modified"
      :selected-model-id="settings.modelId"
      :selected-effort="settings.effort"
      :selected-channel-id="settings.channelId"
      :resolved-channel-id="resolvedChannelId"
      :selected-advisor="settings.advisor"
      :selected-permission-mode="settings.permissionMode"
      @model-change="onModelChange"
      @effort-change="onEffortChange"
      @channel-change="onChannelChange"
      @advisor-change="onAdvisorChange"
      @permission-mode-change="onPermissionModeChange"
      @reload="onReload"
      @deleted="onDeleted"
    />

    <!-- 加载态 -->
    <div v-if="loading" class="flex-1 flex items-center justify-center">
      <p class="text-muted-foreground text-sm">{{ $t('session.loadingChat') }}</p>
    </div>

    <!-- 错误态 -->
    <div v-else-if="error" class="flex-1 flex items-center justify-center">
      <p class="text-destructive text-sm">{{ error }}</p>
    </div>

    <!-- 无记录(草稿会话给引导文案) -->
    <div v-else-if="messages.length === 0 && !stream.streaming && !stream.streamingTurns.length && !stream.pendingUserMessage && !showHelpCard" class="flex-1 flex items-center justify-center">
      <p class="text-muted-foreground text-sm">
        {{ effectiveSessionId && draftCwd(effectiveSessionId) ? $t('session.draftGuide') : $t('session.noRecords') }}
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
        <!-- 渠道切换横线:会话起点的切换(本地记账,jsonl 无渠道信息) -->
        <div
          v-for="(m, j) in channelMarksByUuid.get(null) ?? []"
          :key="`channel-mark-start-${j}`"
          class="channel-mark"
        >
          <div class="flex-1 h-px bg-border" />
          <span class="i-carbon-cloud w-3 h-3" />
          <span>{{ channelMarkLabel(m) }}</span>
          <div class="flex-1 h-px bg-border" />
        </div>
        <!-- 按轮次分组:每组包含一条用户消息 + 后续回复,sticky 限制在组内 -->
        <div
          v-for="(group, gi) in messageGroups"
          :key="group.user?.uuid || `group-${gi}`"
          class="space-y-4"
        >
          <!-- 用户消息:有 AI 回复时吸顶,无回复的短轮次不启用(减少 sticky 元素数量) -->
          <!-- 纯系统注入(无真实用户输入):降级为系统注解样式 -->
          <div v-if="group.user && group.user.type === 'user' && isSystemOnlyUser(group.user)" class="pl-3">
            <MessageBlock
              v-for="(block, bi) in contentBlocks(group.user as any)"
              :key="bi"
              :block="block"
            />
          </div>
          <!-- 正常用户消息 -->
          <div v-else-if="group.user" :class="group.responses.some(r => r.type === 'assistant') ? 'user-msg-sticky' : ''">
            <div class="flex gap-3">
              <div class="w-0.5 shrink-0 rounded-full bg-primary/60" />
              <div class="min-w-0 flex-1 bg-card border border-border rounded px-3 py-2 shadow-paper">
                <div class="text-xs font-medium mb-1 text-primary">{{ $t('session.you') }}</div>
                <MsgClamp>
                  <MessageBlock
                    v-for="(block, bi) in contentBlocks(group.user as any)"
                    :key="bi"
                    :block="block"
                  />
                </MsgClamp>
              </div>
            </div>
          </div>
          <!-- 渠道切换横线:用户消息锚点 -->
          <div
            v-for="(m, j) in (group.user?.uuid ? channelMarksByUuid.get(group.user.uuid) ?? [] : [])"
            :key="`channel-mark-${group.user?.uuid}-${j}`"
            class="channel-mark"
          >
            <div class="flex-1 h-px bg-border" />
            <span class="i-carbon-cloud w-3 h-3" />
            <span>{{ channelMarkLabel(m) }}</span>
            <div class="flex-1 h-px bg-border" />
          </div>
          <!-- 回复(AI + system) -->
          <template v-for="resp in group.responses" :key="resp.uuid || resp">
            <SystemEventRow v-if="resp.type === 'system'" :record="resp" />
            <div v-else class="flex gap-3 msg-block">
              <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
              <div class="min-w-0 flex-1">
                <div class="text-xs font-medium mb-1 text-claude">
                  {{ $t('session.claude') }}
                  <span v-if="(resp as any).message?.model" class="text-muted-foreground font-normal">
                    ({{ shortModel((resp as any).message.model) }})
                  </span>
                </div>
                <div>
                  <MessageBlock
                    v-for="(block, bi) in contentBlocks(resp as any)"
                    :key="bi"
                    :block="block"
                  />
                </div>
              </div>
            </div>
            <!-- 渠道切换横线:回复消息锚点 -->
            <div
              v-for="(m, j) in (resp.uuid ? channelMarksByUuid.get(resp.uuid) ?? [] : [])"
              :key="`channel-mark-${resp.uuid}-${j}`"
              class="channel-mark"
            >
              <div class="flex-1 h-px bg-border" />
              <span class="i-carbon-cloud w-3 h-3" />
              <span>{{ channelMarkLabel(m) }}</span>
              <div class="flex-1 h-px bg-border" />
            </div>
          </template>
        </div>
        <!-- 锚点失效的切换横线兜底:末尾按序渲染,不静默消失 -->
        <div
          v-for="(m, j) in unanchoredChannelMarks"
          :key="`channel-mark-tail-${j}`"
          class="channel-mark"
        >
          <div class="flex-1 h-px bg-border" />
          <span class="i-carbon-cloud w-3 h-3" />
          <span>{{ channelMarkLabel(m) }}</span>
          <div class="flex-1 h-px bg-border" />
        </div>
      </template>

      <!-- 流式区:横幅 + pendingUserMessage + streamingTurns；turns 不主动清(下次 sendMessage 清),横幅位置才稳定 -->
      <div v-if="(interactive && featureBannerShown) || stream.pendingUserMessage || stream.pendingImages?.length || stream.streamingTurns.length || (stream.streaming && stream.streamingTurns.length === 0)" class="space-y-4">
        <SessionBanner
          v-if="interactive && featureBannerShown && effectiveSessionId"
          :session-id="effectiveSessionId"
          :resumed="bannerResumed"
          :cwd="bannerCwd"
          :model="settings.modelId"
          :effort="(settings.effort as string | null)"
          :features="htmlVisualEnabled ? [$t('settings.htmlVisual')] : []"
          :hook-events="bannerHookEvents"
        />
        <div v-if="stream.pendingUserMessage || stream.pendingImages?.length" class="user-msg-sticky">
          <div class="flex gap-3">
            <div class="w-0.5 shrink-0 rounded-full bg-primary/60" />
            <div class="min-w-0 flex-1 bg-card border border-border rounded px-3 py-2 shadow-paper">
              <div class="text-xs font-medium mb-1 text-primary">{{ $t('session.you') }}</div>
              <MsgClamp>
                <div v-if="stream.pendingImages?.length" class="flex gap-2 flex-wrap mb-1">
                  <img
                    v-for="(img, ii) in stream.pendingImages"
                    :key="ii"
                    :src="`data:${img.source.media_type};base64,${img.source.data}`"
                    class="max-w-60 max-h-40 rounded border border-border object-contain"
                  />
                </div>
                <div v-if="stream.pendingUserMessage" class="whitespace-pre-wrap break-words text-sm">{{ stream.pendingUserMessage }}</div>
              </MsgClamp>
            </div>
          </div>
        </div>

        <div v-for="turn in stream.streamingTurns" :key="turn.messageId" class="flex gap-3 msg-block">
          <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
          <div class="min-w-0 flex-1">
            <div class="text-xs font-medium mb-1 text-claude">{{ $t('session.claude') }}</div>
            <TransitionGroup name="block-fade" tag="div" appear>
              <MessageBlock
                v-for="(block, i) in filterConsumedResults(turn.content)"
                :key="i"
                :block="block"
                :streaming="stream.streaming"
              />
            </TransitionGroup>
          </div>
        </div>

        <div v-if="stream.streaming && stream.streamingTurns.length === 0" class="flex gap-3">
          <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
          <div class="text-xs text-muted-foreground flex items-center gap-1.5">
            <span v-if="stream.pendingImages?.length" class="i-carbon-upload w-3 h-3 animate-pulse" />
            {{ stream.pendingImages?.length ? $t('session.sending') : $t('session.thinking') }}
          </div>
        </div>
      </div>

      <div v-if="stream.streamError" class="px-3 py-2 rounded-md bg-destructive/10 text-destructive text-xs">
        {{ stream.streamError }}
      </div>

      <!-- /help 本地帮助卡片 -->
      <SlashHelpCard v-if="showHelpCard" :commands="SLASH_COMMANDS" />

      <!-- 回到底部:用户上滚脱离底部时,贴滚动视口底部 -->
      <div
        v-if="!followStreaming"
        class="sticky bottom-0 flex justify-center pointer-events-none"
      >
        <button
          class="pointer-events-auto px-3 py-1 text-xs rounded-full bg-popover border border-border
                 shadow-paper text-muted-foreground hover:text-foreground transition-colors
                 flex items-center gap-1"
          @click="resumeFollow"
        >
          <span class="i-carbon-arrow-down w-3 h-3" />
          {{ $t('session.backToBottom') }}
        </button>
      </div>
    </div>

    <!-- 工作台列:权限/提问/计划卡片(固定在输入栏上方,按工具分发) -->
    <div
      v-if="interactive && permissionRequest"
      class="px-4 pb-2 shrink-0 flex justify-center"
    >
      <component
        :is="requestCard"
        :key="permissionRequest.requestId"
        :request="permissionRequest"
        @decide="onPermissionDecide"
      />
    </div>

    <!-- 工作台列:输入栏 + 斜杠命令面板(档案馆只读化:整块不渲染;赛马模式由共享输入替代) -->
    <div v-if="interactive && !hideInput && currentSession.summary.cwd" class="px-4 py-3 border-t border-border shrink-0 relative">
      <div v-if="slashError" class="mb-1 text-xs text-destructive">
        {{ slashError }}
      </div>

      <!-- 外部运行跟随提示:CLI 进程在应用外继续跑,期间禁发(同 session 双进程会冲突) -->
      <div v-if="externalRunning" class="mb-1 text-xs text-muted-foreground flex items-center gap-1.5">
        <span class="w-1.5 h-1.5 rounded-full bg-claude animate-pulse shrink-0" />
        {{ $t('session.externalRunning') }}
      </div>

      <SlashCommandPanel
        :visible="slashPanelVisible"
        :query="inputText"
        class="absolute bottom-full left-4 mb-1"
        @select="onSlashSelect"
        @close="onSlashClose"
      />

      <div v-if="stream.pendingQueue.length" class="mb-2 flex flex-col gap-1">
        <div
          v-for="(item, i) in stream.pendingQueue"
          :key="i"
          class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md bg-muted/60 border border-border/50 text-xs group"
        >
          <span class="i-carbon-time w-3 h-3 text-muted-foreground shrink-0" />
          <span class="truncate text-muted-foreground flex-1">{{ item.message }}</span>
          <button
            class="i-carbon-close w-3 h-3 text-muted-foreground/50 hover:text-destructive shrink-0
                   opacity-0 group-hover:opacity-100 transition-opacity"
            :title="$t('common.delete')"
            @click="removePendingQueueItem(effectiveSessionId!, i)"
          />
        </div>
      </div>

      <div v-if="imageInput.images.value.length" class="mb-2 flex gap-2 flex-wrap">
        <div
          v-for="img in imageInput.images.value"
          :key="img.id"
          class="relative w-14 h-14 rounded border border-border overflow-hidden group"
        >
          <img :src="img.dataUrl" class="w-full h-full object-cover" />
          <button
            class="absolute top-0 right-0 w-4 h-4 rounded-bl bg-destructive/80 text-destructive-foreground
                   flex items-center justify-center text-2.5 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
            @click="imageInput.removeImage(img.id)"
          >
            &times;
          </button>
        </div>
      </div>

      <div v-if="imageInput.lastError.value" class="mb-1 text-xs text-destructive">
        {{ imageInput.lastError.value.message }}
      </div>

      <div class="flex items-center gap-2">
        <textarea
          ref="textareaRef"
          v-model="inputText"
          :disabled="externalRunning"
          :placeholder="externalRunning ? $t('session.externalRunningPlaceholder') : $t('session.inputPlaceholder')"
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
          v-if="stream.streaming && !inputText.trim() && !imageInput.images.value.length"
          class="px-3 py-2 text-xs rounded-md bg-accent text-accent-foreground hover:shadow-paper transition-shadow shrink-0"
          @click="onStopStreaming"
        >
          {{ $t('common.stop') }}
        </button>
        <button
          v-else
          :disabled="(!inputText.trim() && !imageInput.images.value.length) || externalRunning"
          class="px-3 py-2 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow shrink-0
                 disabled:opacity-30 disabled:cursor-not-allowed"
          @click="handleSend"
        >
          {{ $t('common.send') }}
        </button>
      </div>
    </div>

    <!-- 档案馆:常驻只读条(FR-009) -->
    <div
      v-if="!interactive"
      class="px-4 py-2 border-t border-border shrink-0 flex items-center gap-2 text-xs text-muted-foreground"
    >
      <span class="i-carbon-document w-3.5 h-3.5 shrink-0" />
      <span v-if="workbenchHome" class="truncate">{{ $t('session.runningInWorkbench', { name: workbenchHome.name }) }}</span>
      <span v-else class="truncate">{{ $t('session.readonlyPreview') }}</span>
      <button
        class="shrink-0 px-2.5 py-1 text-xs rounded-md border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors flex items-center gap-1"
        :disabled="summaryGenerating"
        @click="onGenerateSummary"
      >
        <span v-if="summaryGenerating" class="i-carbon-renew w-3 h-3 animate-spin" />
        <span v-else class="i-carbon-text-short-paragraph w-3 h-3" />
        {{ currentSummary ? $t('archive.refreshSummary') : $t('archive.generateSummary') }}
      </button>
      <button
        class="ml-auto shrink-0 px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow"
        @click="onOpenInWorkbench"
      >
        {{ workbenchHome ? $t('session.goTo') : $t('session.openInWorkbench') }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.msg-block {
  contain: layout style;
}
.user-msg-sticky {
  position: sticky;
  top: 0;
  z-index: 10;
}
/* 渠道切换横线:细分隔线 + 居中小字,本地记账的轻量视觉(非消息气泡) */
.channel-mark {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 10px;
  color: var(--muted-foreground);
  user-select: none;
}
</style>
