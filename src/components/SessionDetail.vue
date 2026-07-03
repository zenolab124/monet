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
  finishedDirty,
} from '@/composables/useStreaming'
import { useSessionSettings, type ChannelMark } from '@/composables/useSessionSettings'
import { useRunConfig } from '@/composables/useRunConfig'
import {
  refreshChannels,
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
import { shortId, shortModel, formatTokens } from '@/types'
import { inferModel } from '@/utils/modelContext'
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
import UserMsgContent from './UserMsgContent.vue'
import { useImageInput } from '@/composables/useImageInput'
import { useHtmlVisual, HTML_VISUAL_PROMPT } from '@/features'
import SessionBanner from './SessionBanner.vue'
import SessionAnchorNav, { type AnchorItem } from './SessionAnchorNav.vue'
import {
  usePermissionRequests,
  currentForSession,
  type RespondExtra,
} from '@/composables/usePermissionRequests'
import { createSubAgentContext } from '@/composables/useSubAgents'
import type { SubAgentMeta } from '@/types'
import SubAgentPanel from './SubAgentPanel.vue'
import { IMAGE_LOCATOR, type ImageLocator } from '@/utils/ccimg'

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

const { sendMessage, stopStreaming, clearStreamingTurns, clearPendingUserMessage, getStream, removePendingQueueItem, consumePendingQueue } = useStreaming()

const { enabled: htmlVisualEnabled } = useHtmlVisual()
const featureBannerShown = ref(false)
const bannerResumed = ref(false)
const bannerCwd = ref('')

// 横幅自动消失:悬浮通知语义——出现后固定停留 BANNER_MS 淡出,与回合进度无关
const BANNER_MS = 5000
let bannerHideTimer = 0
onUnmounted(() => clearTimeout(bannerHideTimer))
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
/** 滚动内容包裹层:布局层滚动跟随的 RO 观察对象(内容总高度的单一载体) */
const scrollContentEl = ref<HTMLElement>()
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
    clearTimeout(bannerHideTimer)
    bannerHideTimer = window.setTimeout(() => { featureBannerShown.value = false }, BANNER_MS)
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
        // recordUuid = tool_result 所在 user record 的 uuid;嵌套图片按此拼协议 URL
        map.set(tr.tool_use_id, { content: tr.content, is_error: tr.is_error, recordUuid: r.uuid })
      }
    }
  }
  for (const turn of stream.value.streamingTurns) {
    for (const b of turn.content) {
      if (b.type === 'tool_result') {
        const tr = b as Extract<ContentBlock, { type: 'tool_result' }>
        // 流式 turns 经 typed 反序列化,图片 data 已剥离;但 assistant 消息不含
        // tool_result,此分支对图片实际不可达——流式期 tool_result 图片由 records
        // 重载走协议 URL。recordUuid 置 null 仅为类型完整
        map.set(tr.tool_use_id, { content: tr.content, is_error: tr.is_error, recordUuid: null })
      }
    }
  }
  return map
})
provide('toolResultMap', toolResultMap)

// --- 子 Agent 侧面板 ---
const {
  subAgentMap,
  openAgents: subAgentTabs,
  panelVisible: subAgentPanelVisible,
  activeTab: subAgentActiveTab,
  activeTabId: subAgentActiveTabId,
  loadSubAgentList,
  findByToolUseId,
  toggleSubAgent,
  closeTab: closeSubAgentTab,
  closeAllTabs: closeAllSubAgents,
  isAgentOpen,
  startPolling,
} = createSubAgentContext()

provide('findSubAgent', (toolUseId: string) => findByToolUseId(toolUseId))
provide('toggleSubAgent', (meta: SubAgentMeta) => toggleSubAgent(meta))
provide('isSubAgentOpen', (agentId: string) => isAgentOpen(agentId))

function hasUnmatchedAgentToolUse(): boolean {
  for (const turn of stream.value.streamingTurns) {
    for (const b of turn.content) {
      if (b.type === 'tool_use' && ((b as any).name === 'Agent' || (b as any).name === 'Task')) {
        if (!subAgentMap.value.has((b as any).id)) return true
      }
    }
  }
  return false
}

startPolling(
  () => stream.value.streaming,
  hasUnmatchedAgentToolUse,
)

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

async function onStop() {
  const sid = effectiveSessionId.value
  if (!sid) return
  if (externalRunning.value) {
    await invoke('kill_external_session', { sessionId: sid })
    return
  }
  await denyAllForSession(sid)
  await stopStreaming(sid)
}

// --- 会话级设置(模型 / 努力等级 / 渠道) ---
const { settings, setModel, setEffort, setChannel, setAdvisor, setPermissionMode: persistPermissionMode } = useSessionSettings(effectiveSessionId)

// 运行配置同源解析:顶栏展示与发送参数共用同一解析结果(会话覆盖 > 渠道默认 > CLI 默认)
const { runConfig } = useRunConfig(settings)

function onModelChange(modelId: string | null) {
  setModel(modelId)
}

function onEffortChange(effort: 'low' | 'medium' | 'high' | 'xhigh' | 'max' | 'ultracode' | null) {
  setEffort(effort)
}

// --- 渠道(per-session 选择 + 切换横线记账) ---

// 渠道名解析(badge/横线)需要清单:实例创建时拉一次,下拉每次打开还会各自重读
refreshChannels()

/** 解析后的最终注入渠道 id(null = 官方/零注入):发送与终端恢复共用 */
const resolvedChannelId = computed(() => runConfig.value.channelId)

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

// 会话级图片定位上下文(主会话,无 agentId);历史区图片按此拼 ccimg 协议 URL
const imageLocator = computed<ImageLocator | null>(() => {
  const cs = currentSession.value
  if (!cs) return null
  return { projectId: cs.projectId, sessionId: cs.summary.id }
})
provide(IMAGE_LOCATOR, imageLocator)

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

const pendingUserBlocks = computed<ContentBlock[]>(() => {
  const blocks: ContentBlock[] = []
  if (stream.value.pendingImages?.length) {
    for (const img of stream.value.pendingImages) {
      blocks.push({ type: 'image', source: img.source } as ContentBlock)
    }
  }
  if (stream.value.pendingUserMessage) {
    blocks.push({ type: 'text', text: stream.value.pendingUserMessage })
  }
  return blocks
})

/** 流式区当前渲染的 message id 集合(用于历史区过滤,避免与流式区重复显示) */
const streamingMessageIds = computed(() =>
  new Set(stream.value.streamingTurns.map(t => t.messageId)),
)

/** 进入消息流的 system 子类型（其余 system 记录为噪音，不渲染） */
const VISIBLE_SYSTEM_SUBTYPES = new Set(['api_error', 'compact_boundary'])

/** 剥离 CLI 落账时并入 user 消息的私有标签注入(hook additionalContext /
 *  system-reminder / command 包装等),留下真实用户文本——落账匹配用。
 *  精确全等对带注入的消息是结构性必然失配,不是偶发。 */
function stripInjections(text: string): string {
  return text.replace(TAG_RE, '')
}

/** 在 recs 中寻找 pending 用户消息对应的落账 user record uuid。
 *  只认发送时刻之后的记录(5s 时钟容差,records 为追加序、扫到更早即停);
 *  文本消息按剥离注入后 trim 相等匹配,纯图片消息按含 image 块匹配。 */
function findLandedUserUuid(
  recs: SessionRecord[],
  pendingText: string | null,
  hasImages: boolean,
  sentAt: number,
): string | null {
  if (!pendingText && !hasImages) return null
  const target = (pendingText ?? '').trim()
  for (let i = recs.length - 1; i >= 0; i--) {
    const r = recs[i]
    if (r.type !== 'user') continue
    const ts = r.timestamp ? Date.parse(r.timestamp) : 0
    if (ts && sentAt && ts < sentAt - 5000) break
    const content = r.message?.content
    if (!content) continue
    if (typeof content === 'string') {
      if (target && stripInjections(content).trim() === target) return r.uuid
      continue
    }
    if (target) {
      const text = content
        .filter((b: ContentBlock) => b.type === 'text')
        .map(b => stripInjections((b as { text?: string }).text ?? ''))
        .join('')
        .trim()
      if (text === target) return r.uuid
    } else if (content.some((b: ContentBlock) => b.type === 'image')) {
      return r.uuid
    }
  }
  return null
}

/** pending 用户消息在历史区的落账 record uuid(落账接管信号)。
 *  非 null = 历史条已可渲染:气泡同帧让位(模板 v-if),watch 随后清理状态。
 *  与旧实现方向相反——旧逻辑匹配成功隐藏历史条、reload 无条件清气泡,
 *  匹配失配 + 误清叠加出「两源皆空」的消息消失窗口。 */
const pendingLandedUuid = computed(() => {
  const s = stream.value
  return findLandedUserUuid(records.value, s.pendingUserMessage, !!s.pendingImages?.length, s.pendingSentAt ?? 0)
})

// 落账接管后清理 pending 状态(显示切换已由 v-if 原子完成,这里只是后勤)
watch(pendingLandedUuid, (uuid) => {
  if (uuid) {
    const sid = effectiveSessionId.value
    if (sid) clearPendingUserMessage(sid)
  }
})

const messages = computed(() => {
  // thinking 耗时标注:在过滤前的原始序列上算(前一行可能是不可见的 tool_result 行)
  annotateThinkingMs(records.value)
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

/** 同 message.id 的连续 assistant 记录合并为单条（CLI 每个 content block 单独写一行） */
function mergeResponses(responses: VisibleRecord[]): VisibleRecord[] {
  const merged: VisibleRecord[] = []
  for (const r of responses) {
    if (r.type !== 'assistant') { merged.push(r); continue }
    const msgId = (r as any).message?.id
    const prev = merged.length ? merged[merged.length - 1] : null
    if (msgId && prev?.type === 'assistant' && (prev as any).message?.id === msgId) {
      const prevMsg = (prev as any).message
      const curMsg = (r as any).message
      prevMsg.content = [...prevMsg.content, ...curMsg.content]
      if (curMsg.usage) prevMsg.usage = curMsg.usage
    } else {
      merged.push({ ...r, message: r.type === 'assistant' && r.message ? { ...r.message, content: [...r.message.content] } : r.message } as any)
    }
  }
  return merged
}

/** 思考耗时标注的合理上限:超过按异常丢弃(跨会话恢复/时钟漂移的脏差值) */
const THINKING_MS_CAP = 600_000

/**
 * 历史区 thinking 块耗时标注:必须在**原始记录序列**上按「与前一行的时间戳差」计算——
 * thinking 行落盘 ≈ 思考结束,前一行落盘 ≈ 思考开始(实测中位 10.8s,与思考时长量级吻合);
 * 「与后一行的差」是下一块的生成间隔(中位 0.6s),曾错标于此导致耗时几乎全被 <1s 显示阈值吞掉。
 * 前一行可能是 tool_result/user 行(过滤后不可见),故不能在 messages/mergeResponses 层算。
 * 幂等:已有 _thinkingMs(流式期 Date.now() 实测值,更准)不覆盖。
 */
function annotateThinkingMs(rows: SessionRecord[]): void {
  let prevTs: number | null = null
  for (const r of rows) {
    const tsStr = (r as any).timestamp as string | null
    const ts = tsStr ? new Date(tsStr).getTime() : NaN
    if (r.type === 'assistant' && prevTs !== null && Number.isFinite(ts)) {
      const content = (r as any).message?.content
      if (Array.isArray(content)) {
        const ms = ts - prevTs
        if (ms > 0 && ms < THINKING_MS_CAP) {
          for (const b of content) {
            if (b?.type === 'thinking' && !b._thinkingMs) b._thinkingMs = ms
          }
        }
      }
    }
    if (Number.isFinite(ts)) prevTs = ts
  }
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
  return groups.map(g => ({ ...g, responses: mergeResponses(g.responses) }))
})

// ---- 滚动锚定补偿（WebKit 无原生 scroll anchoring）----
// 消息组解冻（content-visibility 估算高度→真实高度）与图片按需加载都会改变
// 视口上方内容的总高度；Chrome 有 scroll anchoring 自动补偿，WebKit 没有——
// 表现为滚动中内容"顿一下"。用 ResizeObserver 观察每个消息组，完全位于视口
// 上方的组高度变化时同帧补偿 scrollTop（RO 回调在 layout 后 paint 前，写
// scrollTop 不触发重排，视口内容保持视觉稳定）。
const groupHeights = new WeakMap<Element, number>()
let anchorRO: ResizeObserver | null = null

// ---- 组位置分类（锚定补偿的零布局读数据源）----
// 历史教训（HUD 长帧归因实测）：cv 的手工 hidden/visible 管理在 WebKit 上是
// 负优化（"渲染状态保留"实现不达标，批量切换 = 批量全价 layout，743ms 帧）；
// 补偿回调里读 offsetTop 在布局脏时强制同步 layout（278ms）。故：cv 交还
// 浏览器 auto 自管；组的"是否在视口上方"用 IO 自带几何信息维护（回调 entry
// 的 boundingClientRect/rootBounds 是浏览器附送的，零强制布局读）。
// 分类有一帧异步延迟，边缘组偶发误差可接受。
let posIO: IntersectionObserver | null = null
const groupAbove = new WeakMap<Element, boolean>()

function observeAnchorGroups() {
  const sc = scrollContainer.value
  if (!sc) return
  if (!posIO) {
    posIO = new IntersectionObserver((entries) => {
      for (const e of entries) {
        const above =
          !e.isIntersecting &&
          e.boundingClientRect.bottom <= (e.rootBounds?.top ?? 0)
        groupAbove.set(e.target, above)
      }
    }, { root: sc, threshold: 0 })
  }
  // WeakMap 基线保留：重挂后首次回调 diff=0 不误补偿；断连期间的变化照常补偿
  anchorRO?.disconnect()
  posIO.disconnect()
  for (const el of sc.querySelectorAll('.msg-group-cv')) {
    anchorRO?.observe(el)
    posIO.observe(el)
  }
}

onMounted(() => {
  anchorRO = new ResizeObserver((entries) => {
    const sc = scrollContainer.value
    if (!sc) return
    const perfT0 = performance.now()
    let delta = 0
    for (const entry of entries) {
      const el = entry.target as HTMLElement
      const newH = entry.borderBoxSize?.[0]?.blockSize ?? el.offsetHeight
      const oldH = groupHeights.get(el)
      groupHeights.set(el, newH)
      if (oldH === undefined) continue // 首次观测：建立基线
      const diff = newH - oldH
      if (diff === 0) continue
      // 仅补偿视口上方的组（分类由 posIO 免费维护，本回调零布局属性读——
      // 读 offsetTop 会在布局脏时强制同步 layout，实测 278ms，已废弃该写法）
      if (groupAbove.get(el)) {
        delta += diff
      }
    }
    if (delta !== 0) {
      const before = sc.scrollTop
      sc.scrollTop += delta
      // 校正 onScroll 基线:补偿位移不计入用户手势 delta(负补偿曾被误判为
      // "用户上滚"而静默关闭跟随);clamp 时以实际生效量为准
      lastScrollTop += sc.scrollTop - before
    }
    performance.measure('anchor-comp', { start: perfT0, duration: performance.now() - perfT0 })
  })
  observeAnchorGroups()
})
onUnmounted(() => {
  anchorRO?.disconnect()
  anchorRO = null
  posIO?.disconnect()
  posIO = null
})
watch(messageGroups, () => nextTick(observeAnchorGroups))

function userTextPreview(record: VisibleRecord): string {
  if (record.type !== 'user' || !record.message) return ''
  const content = record.message.content
  if (typeof content === 'string') return content.slice(0, 120)
  const texts: string[] = []
  for (const b of content) {
    if (b.type === 'text' && (b as any).text) {
      const raw = (b as any).text as string
      const clean = raw.replace(/<[^>]+>/g, '').trim()
      if (clean) texts.push(clean)
    }
  }
  return texts.join(' ').slice(0, 120)
}

const anchorItems = computed<AnchorItem[]>(() => {
  const items: AnchorItem[] = []
  for (let i = 0; i < messageGroups.value.length; i++) {
    const g = messageGroups.value[i]
    if (!g.user || g.user.type !== 'user') continue
    if (isSystemOnlyUser(g.user)) continue
    const text = userTextPreview(g.user)
    if (!text) continue
    items.push({ index: i, text })
  }
  return items
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

  // command-name 紧邻 command-args 时合并(args 挂到 name 块),渲染收成一行
  const merged: ContentBlock[] = []
  for (let i = 0; i < results.length; i++) {
    const b = results[i] as any
    const next = results[i + 1] as any
    if (b.type === 'command-name' && next?.type === 'command-args') {
      merged.push({ ...b, args: next.text } as any)
      i++
    } else {
      merged.push(results[i])
    }
  }
  return merged
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

/**
 * 该轮(组)回复是否足够长(组末尾补一行模型/token 标注)。
 * 判定单位必须是整轮而非单条记录:CLI 落盘按 content block 拆行,
 * 一轮长回复 = 多条 assistant 记录、每条常只有 1 个块——按单条判永不触发。
 * 直接读原始 message.content 粗估,不走 contentBlocks 解析(渲染热路径零开销):
 * 全轮块数 ≥ 5(多工具轮必长)或文本/思考字符总量超阈值(约 30+ 行)。
 */
function groupIsLong(group: { responses: unknown[] }): boolean {
  let blocks = 0
  let chars = 0
  for (const r of group.responses) {
    const rec = r as { type?: string; message?: { content?: unknown } }
    if (rec.type !== 'assistant') continue
    const content = rec.message?.content
    if (!Array.isArray(content)) continue
    blocks += content.length
    for (const b of content) {
      if (b?.type === 'text') chars += b.text?.length ?? 0
      else if (b?.type === 'thinking') chars += b.thinking?.length ?? 0
      else chars += 200
    }
    if (blocks >= 5 || chars > 1800) return true
  }
  return false
}

/**
 * 组末尾标注整行文案(长轮次才有):数据取该轮最后一条有效 assistant 记录,
 * 模型/token 与顶部标注同源。返回 null = 不渲染。
 */
function groupFooterText(group: { responses: unknown[] }): string | null {
  if (!groupIsLong(group)) return null
  for (let i = group.responses.length - 1; i >= 0; i--) {
    const rec = group.responses[i] as { type?: string; message?: { model?: string; usage?: Record<string, number> } }
    if (rec.type === 'assistant' && rec.message?.model && rec.message.model !== '<synthetic>') {
      const parts = [shortModel(rec.message.model)]
      const u = rec.message.usage
      if (u) {
        parts.push(
          `${formatTokens(u.input_tokens ?? 0)} in`,
          `${formatTokens(u.cache_read_input_tokens ?? 0)} cache`,
          `${formatTokens(u.cache_creation_input_tokens ?? 0)} new`,
          `${formatTokens(u.output_tokens ?? 0)} out`,
        )
      }
      return parts.join(' · ')
    }
  }
  return null
}

const USER_CONTENT_TYPES = new Set(['text', 'image', 'document'])

function isSystemOnlyUser(record: Extract<SessionRecord, { type: 'user' }>): boolean {
  const blocks = contentBlocks(record as any)
  return blocks.length > 0 && blocks.every(b => !USER_CONTENT_TYPES.has(b.type))
}

/** 用户卡是否有可见内容:全空白时不渲染空卡壳(纯图片/文档消息不受影响) */
function userHasVisibleContent(record: Extract<SessionRecord, { type: 'user' }>): boolean {
  const blocks = contentBlocks(record as any)
  return blocks.some(b =>
    b.type === 'image' || b.type === 'document' || (b.type === 'text' && !!(b as any).text?.trim()),
  )
}

/**
 * /model 切换事件的横线文案:以 stdout 记录「Set model to X」为事实源——
 * 它是 CLI 确认执行成功才落的输出,有参/无参/取消三种场景天然正确
 * (取消时无 stdout,不留假横线)。文案不匹配时返回 null,安全降级为普通 stdout 行。
 */
function modelSwitchName(record: Extract<SessionRecord, { type: 'user' }>): string | null {
  const blocks = contentBlocks(record as any)
  for (const b of blocks) {
    if (b.type === 'local-command-stdout') {
      const m = (((b as any).text as string) ?? '').match(/^Set model to\s+(.+)$/)
      if (m) {
        const raw = m[1].trim()
        return inferModel(raw)?.label ?? raw
      }
    }
  }
  return null
}

/** /model 命令记录本身(胶囊+参数):静默不渲染,事件由 stdout 横线承载 */
function isModelCommandRecord(record: Extract<SessionRecord, { type: 'user' }>): boolean {
  const blocks = contentBlocks(record as any)
  const cmd = blocks.find(b => b.type === 'command-name')
  return !!cmd && (((cmd as any).text as string) ?? '').trim() === '/model'
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
  // 发送前重读渠道清单(活文件),runConfig 随之解析出最新的渠道默认
  await refreshChannels()
  const rc = runConfig.value
  const opts = {
    model: rc.model,
    effort: rc.effort ?? null,
    channel: rc.channelId,
    advisor,
    images,
    permissionMode: settings.value.permissionMode,
  }
  if (externalRunning.value) {
    stream.value.pendingQueue.push({ message: text, opts })
    return
  }
  await sendMessage(cs.summary.id, cs.summary.cwd, text, opts)
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
let programmaticTimer = 0

/** 最近一次滚轮上滚意图时刻:窗口期内 contentRO 暂停贴底。
 *  没有它,用户上滚与打字机/图片增高同帧竞争时,RO 在 layout 后把位置贴回、
 *  onScroll 事后读到的已是贴底值——脱离手势被吞,表现为"滚不上去被拽回" */
let wheelUpIntentAt = 0

function onScrollWheel(e: WheelEvent) {
  if (e.deltaY < -3) {
    wheelUpIntentAt = performance.now()
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
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
    if (performance.now() - resumedAt < 300) return
    if (delta < 0 && distFromBottom > 5) {
      followStreaming.value = false
    } else if (delta > 0 && !followStreaming.value) {
      // 恢复阈值必须窄:曾是 max(半屏,400),触控板惯性衰减的微小下滑就会在
      // 距底几百 px 处误恢复跟随,contentRO 随即贴底=「还差几行被强制吸走」。
      // 收紧为"几乎滚到底才算想回去",滚到底本身另有 dist<2 兜底
      if (distFromBottom < 48) {
        followStreaming.value = true
        resumedAt = performance.now()
      }
    }
    // 兜底：到底部就恢复，不依赖 delta 方向
    if (!followStreaming.value && distFromBottom < 2) {
      followStreaming.value = true
      resumedAt = performance.now()
    }
  })
}

function resumeFollow() {
  followStreaming.value = true
  resumedAt = performance.now()
  wheelUpIntentAt = 0
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
      clearTimeout(programmaticTimer)
      programmaticTimer = window.setTimeout(() => { programmaticScroll = false }, 50)
      el.scrollTop = el.scrollHeight
      // 内容刚挂载时 scrollHeight 可能还是 0，延迟重试一次
      if (el.scrollHeight <= el.clientHeight) {
        requestAnimationFrame(() => {
          programmaticScroll = true
          clearTimeout(programmaticTimer)
          programmaticTimer = window.setTimeout(() => { programmaticScroll = false }, 50)
          el.scrollTop = el.scrollHeight
        })
      }
    })
  })
}

watch(() => stream.value.pendingUserMessage, (val) => {
  if (val) scrollToBottom(true)
})

// ---- 布局层滚动跟随(水平触发) ----
// 跟随不再挂数据事件(watch streamingTick/records 已删),改挂"内容高度变化"本身:
// 打字机、晚到子 Agent turn、records 落账替换、图片加载、cv 组解冻、横幅出现,
// 任何增高源统一在此贴底——新增数据链路无需再记得挂滚动。
// RO 回调在 layout 后 paint 前执行:读 scrollHeight 无强制布局,写 scrollTop 同帧生效。
let contentRO: ResizeObserver | null = null
watch(scrollContentEl, (el) => {
  contentRO?.disconnect()
  if (!el) return
  if (!contentRO) {
    contentRO = new ResizeObserver(() => {
      if (!followStreaming.value) return
      // 用户刚有上滚意图:让手势先走,onScroll/onScrollWheel 正常完成脱离判定
      if (performance.now() - wheelUpIntentAt < 150) return
      const sc = scrollContainer.value
      if (!sc) return
      const target = sc.scrollHeight - sc.clientHeight
      if (sc.scrollTop < target) {
        sc.scrollTop = target
        // 同步基线:贴底位移不计入 onScroll 的手势 delta,不会被误判为用户滚动
        lastScrollTop = sc.scrollTop
      }
    })
  }
  contentRO.observe(el)
}, { immediate: true })
onUnmounted(() => { contentRO?.disconnect(); contentRO = null })

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
    // pending 用户消息是否已在 recs 中落账(无 pending 视为已落账,不阻塞)。
    // 用 sid 对应 state 而非 stream.value:守卫运行在异步窗口,用户可能已切会话
    const pendingLanded = (recs: SessionRecord[] | null) => {
      const s = getStream(sid)
      if (!s.pendingUserMessage && !s.pendingImages?.length) return true
      return !!recs && !!findLandedUserUuid(recs, s.pendingUserMessage, !!s.pendingImages?.length, s.pendingSentAt ?? 0)
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
    // 重试条件加"用户消息尚未落账":旧守卫只看总数增长,assistant 侧落账也会
    // 使总数增长,曾把"用户 record 还没写进 jsonl"误判为 reload 成功
    if (!newRecords || newRecords.length === records.value.length || !pendingLanded(newRecords)) {
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
    // keepPending:气泡退场由落账匹配驱动(pendingLandedUuid watch),重试尽头仍未
    // 落账时保留气泡等下一轮 reload——宁可气泡多活一会,不可消息凭空消失
    clearStreamingTurns(sid, { keepPending: true })
    if (cs.summary.cwd) consumePendingQueue(sid, cs.summary.cwd)
  }
})

// 滚动跟随已移交布局层 contentRO(见上方"布局层滚动跟随"):
// 旧实现 watch(streamingTick) 带 streaming===true 守卫,回合结束后晚到的
// 子 Agent 事件渲染时恒短路(Bug:不跟随滚动),且每 tick 读 scrollHeight
// 是强制布局热点。contentRO 对增高源一视同仁,无此两病。

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
      // 进程在跑就保持运行态,不做闲置退出(API 调用等响应可能 10-30s 无输出)
      // 恢复跟随必须先于首批 reload:否则 watch 链在 follow=false 时错过唯一一发
      // 贴底,且 false→true 沿过后不再有人补滚(resumeFollow 自带补滚,时序免疫)
      if (!externalRunning.value) {
        externalRunning.value = true
        resumeFollow()
      }
      const prevCount = records.value.length
      await silentReloadRecords()
      if (records.value.length > prevCount) {
        externalIdleTicks = 0
      }
    } else if (externalRunning.value) {
      // 进程退出:收尾 reload 拿最终落账,结束跟随,消费排队消息
      externalRunning.value = false
      externalIdleTicks = 0
      await silentReloadRecords()
      stopExternalFollow()
      const cs = currentSession.value
      if (cs?.summary.cwd) consumePendingQueue(effectiveSessionId.value!, cs.summary.cwd)
    } else {
      // 进程未运行且从未标记过运行态,累积空轮次后停止探测
      externalIdleTicks++
      if (externalIdleTicks >= 4) {
        stopExternalFollow()
      }
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

// 文件变化驱动探测：watcher 检测到 JSONL 增长时触发 projects-changed，
// 如果当前会话探测已停止且未在本地流式，重启探测以捕获孤儿进程输出
let unlistenProjectsChanged: (() => void) | null = null
listen('projects-changed', () => {
  if (externalTimer !== null) return
  if (!currentSession.value || stream.value.streaming) return
  startExternalFollow()
}).then(fn => { unlistenProjectsChanged = fn })
onUnmounted(() => unlistenProjectsChanged?.())

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
        closeAllSubAgents()
        await loadRecords(cs.projectId, cs.summary.id, force)
        loadSubAgentList(cs.projectId, cs.summary.id)
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
    startExternalFollow()
  }
}
</script>

<template>
  <!-- 空态 -->
  <div v-if="!currentSession" class="h-full flex items-center justify-center">
    <p class="text-muted-foreground text-sm">{{ mode === 'workbench' ? $t('session.notExist') : $t('archive.selectSession') }}</p>
  </div>

  <div v-else class="h-full flex min-h-0">
    <!-- 主内容区 -->
    <div class="flex-1 min-w-0 flex flex-col">
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
      :total-tokens="currentSession.summary.total_tokens"
      :last-modified="currentSession.summary.last_modified"
      :selected-model-id="settings.modelId"
      :selected-effort="settings.effort"
      :selected-channel-id="settings.channelId"
      :resolved-channel-id="resolvedChannelId"
      :run-config="runConfig"
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
    <div v-else class="flex-1 min-h-0 relative">
    <SessionAnchorNav
      :anchors="anchorItems"
      :scroll-container="scrollContainer"
    />
    <!-- 会话横幅:悬浮通知层,不占文档流——出现/增高不推挤消息、不触发滚动跟随,
         根除"hook 事件陆续到达时横幅增高顶块/与用户上滚手势竞态拽回"。
         最短停留 5s,首轮回合结束后到点淡出(scheduleBannerHide) -->
    <Transition name="banner-float">
      <div
        v-if="interactive && featureBannerShown && effectiveSessionId"
        class="absolute top-2 left-4 right-4 z-20 pointer-events-none"
      >
        <div class="pointer-events-auto shadow-paper-lifted rounded-md bg-popover/40 backdrop-blur-md border border-border">
          <SessionBanner
            :session-id="effectiveSessionId"
            :resumed="bannerResumed"
            :cwd="bannerCwd"
            :model="settings.modelId"
            :effort="(settings.effort as string | null)"
            :features="htmlVisualEnabled ? [$t('settings.htmlVisual')] : []"
            :hook-events="bannerHookEvents"
          />
        </div>
      </div>
    </Transition>
    <div
      ref="scrollContainer"
      class="h-full overflow-y-auto min-h-0 px-4 py-3 overscroll-contain relative"
      @wheel.passive="onScrollWheel"
      @scroll.passive="onScroll"
    >
    <!-- 内容包裹层:所有增高源(打字机/晚到turn/落账替换/图片/cv解冻/横幅)都反映为它的高度变化,
         contentRO 观察它实现水平触发的滚动跟随 -->
    <div ref="scrollContentEl" class="space-y-4">
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
          :data-anchor-index="gi"
          class="space-y-4 msg-group-cv"
        >
          <!-- 用户消息:有 AI 回复时吸顶,无回复的短轮次不启用(减少 sticky 元素数量) -->
          <!-- /model 切换成功(stdout 事实源):渲染成与渠道切换同款的配置分界横线 -->
          <div
            v-if="group.user && group.user.type === 'user' && modelSwitchName(group.user)"
            class="channel-mark"
          >
            <div class="flex-1 h-px bg-border" />
            <span class="i-carbon-model-alt w-3 h-3" />
            <span>{{ $t('session.modelSwitchMark', { name: modelSwitchName(group.user) }) }}</span>
            <div class="flex-1 h-px bg-border" />
          </div>
          <!-- /model 命令记录本身:静默(事件由上面的 stdout 横线承载;取消选择时无 stdout,不留痕) -->
          <template v-else-if="group.user && group.user.type === 'user' && isModelCommandRecord(group.user)" />
          <!-- 纯系统注入(无真实用户输入):降级为系统注解样式 -->
          <div v-else-if="group.user && group.user.type === 'user' && isSystemOnlyUser(group.user)" class="pl-3">
            <MessageBlock
              v-for="(block, bi) in contentBlocks(group.user as any)"
              :key="bi"
              :block="block"
              :record-uuid="group.user.uuid"
            />
          </div>
          <!-- 正常用户消息(全空白内容不渲染空卡壳) -->
          <div v-else-if="group.user && group.user.type === 'user' && userHasVisibleContent(group.user)" :class="group.responses.some(r => r.type === 'assistant') ? 'user-msg-sticky' : ''">
            <div class="flex gap-3">
              <div class="w-0.5 shrink-0 rounded-full bg-primary/60" />
              <div class="min-w-0 flex-1 bg-card border border-border rounded px-3 py-2 shadow-paper">
                <div class="text-xs font-medium mb-1 text-primary">{{ $t('session.you') }}</div>
                <MsgClamp>
                  <UserMsgContent :blocks="contentBlocks(group.user as any)" :record-uuid="group.user.uuid" />
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
                <div class="text-xs font-medium mb-1 text-claude flex items-center gap-1.5 flex-wrap">
                  <span>
                    {{ $t('session.claude') }}
                    <span v-if="(resp as any).message?.model" class="text-muted-foreground font-normal">
                      ({{ shortModel((resp as any).message.model) }})
                    </span>
                  </span>
                  <span v-if="(resp as any).message?.usage" class="text-muted-foreground/70 font-normal tabular-nums">
                    {{ formatTokens((resp as any).message.usage.input_tokens) }} in
                    · {{ formatTokens((resp as any).message.usage.cache_read_input_tokens) }} cache
                    · {{ formatTokens((resp as any).message.usage.cache_creation_input_tokens) }} new
                    · {{ formatTokens((resp as any).message.usage.output_tokens) }} out
                  </span>
                </div>
                <div>
                  <MessageBlock
                    v-for="(block, bi) in contentBlocks(resp as any)"
                    :key="bi"
                    :block="block"
                    :record-uuid="(resp as any).uuid"
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
          <!-- 长轮次组末尾补一行模型/token 标注:滚到底不用回头找归属(数据取该轮最后一条 assistant,与顶部标注同源) -->
          <div
            v-if="groupFooterText(group)"
            class="pl-3.5 text-[11px] text-muted-foreground/70 tabular-nums"
          >
            {{ groupFooterText(group) }}
          </div>
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

      <!-- 流式区:pendingUserMessage + streamingTurns(横幅已移出文档流,悬浮层见上方) -->
      <div v-if="stream.pendingUserMessage || stream.pendingImages?.length || stream.streamingTurns.length || (stream.streaming && stream.streamingTurns.length === 0)" class="space-y-4">
        <!-- 落账接管即让位:pendingLandedUuid 非 null 时历史条与气泡同帧原子切换,无双显无空窗 -->
        <div v-if="(stream.pendingUserMessage || stream.pendingImages?.length) && !pendingLandedUuid" class="user-msg-sticky">
          <div class="flex gap-3">
            <div class="w-0.5 shrink-0 rounded-full bg-primary/60" />
            <div class="min-w-0 flex-1 bg-card border border-border rounded px-3 py-2 shadow-paper">
              <div class="text-xs font-medium mb-1 text-primary">{{ $t('session.you') }}</div>
              <MsgClamp>
                <UserMsgContent :blocks="pendingUserBlocks" />
              </MsgClamp>
            </div>
          </div>
        </div>

        <div v-for="turn in stream.streamingTurns" :key="turn.messageId" class="flex gap-3 msg-block">
          <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
          <div class="min-w-0 flex-1">
            <div class="text-xs font-medium mb-1 text-claude flex items-center gap-1.5">
              <span>
                {{ $t('session.claude') }}
                <!-- 本轮实际运行模型的真值(message_start 回显),从首字起与落账后标注同源 -->
                <span v-if="turn.model" class="text-muted-foreground font-normal">({{ shortModel(turn.model) }})</span>
              </span>
              <span v-if="!stream.streaming && stream.realUsedTokens" class="text-muted-foreground/70 font-normal tabular-nums">
                {{ formatTokens(stream.realUsedTokens) }} in
                <template v-if="stream.realOutputTokens"> · {{ formatTokens(stream.realOutputTokens) }} out</template>
              </span>
            </div>
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

      <div v-if="stream.streaming || externalRunning" class="flex items-center gap-1 py-2 pl-5">
        <span class="typing-dot" /><span class="typing-dot" /><span class="typing-dot" />
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

      <!-- 外部运行跟随提示 -->
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
          :placeholder="$t('session.inputPlaceholder')"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-popover border border-border
                 text-foreground placeholder-muted-foreground resize-none
                 focus:outline-none focus:border-ring transition-colors"
          @keydown="onInputKeydown"
          @input="onInputChange"
          @keyup="syncCursor"
          @click="syncCursor"
          @select="syncCursor"
        />
        <button
          v-if="(stream.streaming || externalRunning) && !inputText.trim() && !imageInput.images.value.length"
          class="px-3 py-2 text-xs rounded-md bg-accent text-accent-foreground hover:shadow-paper transition-shadow shrink-0"
          @click="onStop"
        >
          {{ $t('common.stop') }}
        </button>
        <button
          v-else
          :disabled="!inputText.trim() && !imageInput.images.value.length"
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
    <!-- 子 Agent 侧面板 -->
    <Transition name="subagent-slide">
      <div
        v-if="subAgentPanelVisible"
        class="shrink-0 border-l border-border overflow-hidden"
        :style="{ width: '45%', maxWidth: '520px' }"
      >
        <SubAgentPanel
          :tabs="subAgentTabs"
          :active-tab-id="subAgentActiveTabId"
          :project-id="currentSession?.projectId ?? null"
          :session-id="currentSession?.summary.id ?? null"
          @select="subAgentActiveTabId = $event"
          @close-tab="closeSubAgentTab($event)"
          @close-all="closeAllSubAgents()"
        />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* 会话横幅悬浮层:淡入下滑进场,淡出上滑退场 */
.banner-float-enter-active,
.banner-float-leave-active {
  transition: opacity 200ms ease, transform 200ms ease;
}
.banner-float-enter-from,
.banner-float-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
.subagent-slide-enter-active,
.subagent-slide-leave-active {
  transition: width 220ms cubic-bezier(0.32, 0.72, 0, 1), opacity 220ms ease;
  overflow: hidden;
}
.subagent-slide-enter-from,
.subagent-slide-leave-to {
  width: 0 !important;
  opacity: 0;
}
.msg-block {
  contain: layout style;
}
/* 消息组级按需渲染:屏外轮次跳过 style/layout/paint,把横滚整列解冻的尖峰
   降为单轮次粒度(审计 P1-2 第一步;列级 content-visibility 在 SortableColumn 互补保留)。
   auto 关键字记住实际渲染高度,首次以 300px 估算——组高度差异大,记忆后滚动条稳定 */
.msg-group-cv {
  content-visibility: auto;
  contain-intrinsic-size: auto 300px;
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
.typing-dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--muted-foreground);
  opacity: 0.4;
  animation: typing-blink 1.4s infinite both;
}
.typing-dot:nth-child(2) { animation-delay: 0.2s; }
.typing-dot:nth-child(3) { animation-delay: 0.4s; }
@keyframes typing-blink {
  0%, 80%, 100% { opacity: 0.15; }
  40% { opacity: 0.6; }
}
</style>
