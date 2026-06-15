import { reactive, computed, ref, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ContentBlock } from '@/types'
import { triggerMetaGeneration } from './useSessionMeta'
import type { EffortSetting } from './useSessionSettings'
import { frameWatchRetain, frameWatchRelease, probeFinishFlip } from '@/utils/perfProbe'
import i18n from '../locales'

export interface SendOptions {
  model?: string
  /** null/缺省 = 跟随 CLI(不附加 --effort);'ultracode' 经 --settings 注入 */
  effort?: EffortSetting
  /**
   * 已解析的最终渠道 id(resolveChannel 产物):null/缺省 = 零注入走官方。
   * 解析值进 lastSent 一起缓存——重试必须复用发送时的渠道,不受期间默认值变化影响
   */
  channel?: string | null
  /** 顾问模式:true 时经 --settings 注入 advisorModel + env flag(主模型由调用方强制为 sonnet) */
  advisor?: boolean
  /** Anthropic image content blocks(base64 编码) */
  images?: Array<{ type: 'image'; source: { type: 'base64'; media_type: string; data: string } }>
}

export interface StreamingTurn {
  messageId: string
  content: ContentBlock[]
}

/** 监控卡尾部行：普通文本 / 等宽工具行 / 错误行 */
export interface TailLine {
  kind: 'text' | 'tool' | 'error'
  text: string
}

/**
 * 单会话流式状态（v2.1.0 per-session 化：多会话并行流式互不阻塞）。
 * tail/activeTool 供工作台左列监控卡消费——监控卡只订阅这两个轻量字段，
 * 不 mount 消息流组件树（NFR-001 渲染分级的结构性前提）。
 */
export interface SessionStreamState {
  streaming: boolean
  streamingTurns: StreamingTurn[]
  pendingUserMessage: string | null
  streamError: string | null
  /** 本次流式开始时刻（监控卡持续时间显示） */
  startedAt: number | null
  /** 进行中的工具名（block_stop(tool_use) 后等待执行；「等待工具/Workflow」状态判定） */
  activeTool: string | null
  /** 尾部行（150ms 合并刷新） */
  tail: TailLine[]
  /** 最近一次发送的消息（出错重试用） */
  lastSent: { cwd: string; message: string; opts: SendOptions } | null
  /** API 报告的真实上下文容量（result 事件 modelUsage.contextWindow） */
  realContextWindow: number | null
  /** API 报告的已用 input token 数（result 事件 modelUsage.inputTokens） */
  realUsedTokens: number | null
  /** Remote Control 是否已启用（open_session 自动启用，手动可关） */
  rcActive: boolean
}

interface BlockDeltaPayload {
  type: 'text_delta' | 'input_json_delta' | 'thinking_delta' | 'signature_delta' | string
  text?: string
  partial_json?: string
  thinking?: string
  signature?: string
}

interface StreamEventPayload {
  kind: 'assistant_message' | 'block_start' | 'block_delta' | 'block_stop' | 'result' | 'error'
  session_id: string
  message_id?: string
  content?: ContentBlock[]
  // block_start / block_delta / block_stop
  index?: number
  content_block?: ContentBlock
  delta?: BlockDeltaPayload
  // result
  text?: string
  cost_usd?: number
  context_window?: number
  input_tokens?: number
  output_tokens?: number
  // error
  message?: string
}

// ---- per-session 状态表 ----

const streams = reactive(new Map<string, SessionStreamState>())

function createState(): SessionStreamState {
  return {
    streaming: false,
    streamingTurns: [],
    pendingUserMessage: null,
    streamError: null,
    startedAt: null,
    activeTool: null,
    tail: [],
    lastSent: null,
    realContextWindow: null,
    realUsedTokens: null,
    rcActive: false,
  }
}

/** 取（或建）某会话的流式状态 */
export function getStream(sessionId: string): SessionStreamState {
  let s = streams.get(sessionId)
  if (!s) {
    // set 后必须重新 get：reactive Map 的 get 才返回深层 reactive 代理,
    // 直接返回原始对象会让后续 mutate 绕过响应式
    streams.set(sessionId, createState())
    s = streams.get(sessionId)!
  }
  return s
}

const EMPTY_STATE: SessionStreamState = createState()

/** 组件侧：按响应式 sessionId 取当前会话的流式状态（无状态时返回冻结空态） */
export function useSessionStream(
  sessionId: Ref<string | null> | ComputedRef<string | null>,
) {
  return computed<SessionStreamState>(() => {
    const sid = sessionId.value
    if (!sid) return EMPTY_STATE
    return streams.get(sid) ?? EMPTY_STATE
  })
}

/** 是否有任一会话正在流式（全局视图用） */
export function anyStreaming(): boolean {
  for (const s of streams.values()) {
    if (s.streaming) return true
  }
  return false
}

// ---- 打字机批 flush（全局单队列，key=messageId#index 天然多流安全） ----

// 累积每个 (messageId, index) 上 tool_use 的 partial_json 字符串,
// block_stop 时 JSON.parse 一次性写入 block.input
const toolPartialJson = new Map<string, string>()
const partialJsonKey = (messageId: string, index: number) => `${messageId}#${index}`

// messageId → { turn, sessionId }：flush 时 O(1) 反查（跨会话）
const turnIndex = new Map<string, { turn: StreamingTurn; sessionId: string }>()

/** 丢弃某 turn 的全部传输态(索引 + 残余 pending):防死 key 让节奏档位虚高 */
function dropTurnTransients(messageId: string) {
  turnIndex.delete(messageId)
  const prefix = `${messageId}#`
  for (const key of pendingTextDeltas.keys()) {
    if (key.startsWith(prefix)) pendingTextDeltas.delete(key)
  }
  for (const key of toolPartialJson.keys()) {
    if (key.startsWith(prefix)) toolPartialJson.delete(key)
  }
}

// RAF 批 flush:字符级 delta 高频到达,直接 mutate 会让 BlockText 的 markdown-it+shiki
// 重渲染 100+ 次/秒,导致卡顿且滚动跟随 watcher 无法识别字符级变化。
// 策略:text/thinking/signature delta 先累到 pendingTextDeltas,每帧合并 flush 一次到 block。
// streamingTick 每帧递增一次,SessionDetail watch 它即可统一处理滚动跟随。
// 渲染开销天然按订阅分级:无挂载组件的会话(仅左列/非激活 tab)更新 block 无人订阅,零渲染成本。
interface PendingTextDelta {
  text?: string
  thinking?: string
  signature?: string
}
const pendingTextDeltas = new Map<string, PendingTextDelta>()
export const streamingTick = ref(0)
let rafId: number | null = null

/**
 * 自适应打字间隔(ms):积压越深间隔越短,但下限锁在帧级(16ms)——
 * 渲染频率永不超过 60fps(markdown 重渲染是主要开销,超帧率只添卡顿不添平滑),
 * 更深的积压靠 charsPerTick 每帧多吐字排空。积压量只计 text(thinking 不逐字)。
 */
function typingInterval(queueLen: number): number {
  if (queueLen <= 3) return 80    // ~12 字/秒,慢节奏(接近正常朗读)
  if (queueLen <= 15) return 40   // ~25 字/秒,中节奏
  return 16                       // 帧级,~60 字/秒起,排空提速走多字
}

/** 每帧吐字数:排空速度的提升手段(间隔已锁帧级)。多字成组出现在帧级仍是平滑的 */
function charsPerTick(queueLen: number): number {
  if (queueLen <= 50) return 1    // ~60 字/秒
  if (queueLen <= 200) return 3   // ~188 字/秒,追赶 burst
  return 8                        // ~500 字/秒,巨量积压(整段长文一次性到达)
}

function totalPendingChars(): number {
  let total = 0
  pendingTextDeltas.forEach(p => {
    total += p.text?.length ?? 0
  })
  return total
}

/**
 * @param allAtOnce    true:一次性 flush 全部(流结束 / 中断收尾用,确保不丢字符)
 *                     false:打字机节奏——每会话每帧只播最早未完成的 text 块
 *                     (块序播放,多会话互不阻塞);thinking/signature 到帧即全量落地
 * @param onlySession  仅 flush 指定会话的 pending(某一路流结束时不影响其他在播会话)
 * @returns 是否还有未消化字符
 */
function flushTextDeltas(allAtOnce = false, onlySession?: string): boolean {
  if (pendingTextDeltas.size === 0) return false
  let hasRemaining = false
  const take = allAtOnce ? Infinity : charsPerTick(totalPendingChars())
  // 本帧已播过 text 的会话:同会话更靠后的 text 块等前块播完(保持阅读顺序)
  const playedText = new Set<string>()
  pendingTextDeltas.forEach((p, key) => {
    const hashIdx = key.indexOf('#')
    if (hashIdx < 0) return
    const mid = key.slice(0, hashIdx)
    const index = parseInt(key.slice(hashIdx + 1), 10)
    const entry = turnIndex.get(mid)
    if (!entry) return
    if (onlySession && entry.sessionId !== onlySession) {
      hasRemaining = true
      return
    }
    const block = entry.turn.content[index] as ContentBlock | undefined
    if (!block) return

    // thinking 默认折叠,逐字播放无意义且会把节奏档位顶满:到帧全量落地
    if (p.thinking !== undefined && block.type === 'thinking') {
      ;(block as { thinking: string }).thinking += p.thinking
      p.thinking = undefined
    }
    if (p.signature !== undefined && block.type === 'thinking') {
      ;(block as Record<string, unknown>).signature = p.signature
      p.signature = undefined
    }

    if (p.text !== undefined && block.type === 'text') {
      if (!allAtOnce && playedText.has(entry.sessionId)) {
        hasRemaining = true
      } else {
        ;(block as { text: string }).text += p.text.slice(0, take)
        const rem = p.text.slice(take)
        if (rem) {
          p.text = rem
          hasRemaining = true
        } else {
          p.text = undefined
        }
        playedText.add(entry.sessionId)
      }
    }

    if (p.text === undefined && p.thinking === undefined && p.signature === undefined) {
      pendingTextDeltas.delete(key)
    }
  })
  return hasRemaining
}

function bump() {
  if (rafId !== null) return
  const delay = typingInterval(totalPendingChars())
  rafId = window.setTimeout(() => {
    rafId = null
    const hasRemaining = flushTextDeltas(false)
    streamingTick.value++
    if (hasRemaining) bump()
  }, delay)
}

// ---- 尾部状态聚合（监控卡消费,150ms 合并;FR-003） ----

/** 各会话的原始文本尾巴累积（不走打字机,收到即累,150ms 刷成 tail 行） */
const tailTextAcc = new Map<string, string>()
const tailDirty = new Set<string>()
let tailTimer: number | null = null
const TAIL_FLUSH_MS = 150
const TAIL_KEEP_CHARS = 400

function markTailDirty(sessionId: string) {
  tailDirty.add(sessionId)
  if (tailTimer !== null) return
  tailTimer = window.setTimeout(() => {
    tailTimer = null
    tailDirty.forEach(sid => rebuildTail(sid))
    tailDirty.clear()
  }, TAIL_FLUSH_MS)
}

/** 重建某会话的 tail 行：最近工具行（若有）在上 + 文本末 2 行 */
function rebuildTail(sessionId: string) {
  const state = streams.get(sessionId)
  if (!state) return
  const lines: TailLine[] = []
  if (state.activeTool) {
    lines.push({ kind: 'tool', text: state.activeTool })
  }
  const acc = tailTextAcc.get(sessionId) ?? ''
  const textLines = acc.split('\n').map(l => l.trim()).filter(Boolean)
  for (const l of textLines.slice(-2)) {
    lines.push({ kind: 'text', text: l.length > 60 ? l.slice(-60) : l })
  }
  if (state.streamError) {
    lines.push({ kind: 'error', text: state.streamError.slice(0, 80) })
  }
  state.tail = lines.slice(-3)
}

function accumulateTailText(sessionId: string, text: string) {
  const prev = tailTextAcc.get(sessionId) ?? ''
  const next = (prev + text).slice(-TAIL_KEEP_CHARS)
  tailTextAcc.set(sessionId, next)
  markTailDirty(sessionId)
}

/** 从工具 input 提取摘要目标（file_path/command/url/pattern 首个字符串） */
function toolTarget(input: Record<string, unknown> | undefined): string | null {
  if (!input) return null
  for (const k of ['file_path', 'command', 'url', 'pattern', 'skill', 'prompt']) {
    const v = input[k]
    if (typeof v === 'string' && v) {
      const s = v.split('\n')[0]
      return s.length > 36 ? `${s.slice(0, 36)}…` : s
    }
  }
  return null
}

/**
 * 快照块能否合并进某个流式块:类型相同之外还须内容兼容——流式累积必是快照
 * 终态的前缀,否则它是同类型的另一个块(interleaved thinking / 多 text 块场景),
 * 应继续向后找或 append。快照内容字段为空(redacted thinking)视为兼容。
 * tool_use 有唯一 id,直接比对。
 */
function snapshotMatches(current: ContentBlock, incoming: ContentBlock): boolean {
  if (current.type !== incoming.type) return false
  const compat = (cur: unknown, inc: unknown): boolean => {
    if (typeof inc !== 'string' || inc.length === 0) return true
    return typeof cur === 'string' && inc.startsWith(cur)
  }
  if (incoming.type === 'text') {
    return compat((current as { text?: string }).text, (incoming as { text?: string }).text)
  }
  if (incoming.type === 'thinking') {
    return compat(
      (current as { thinking?: string }).thinking,
      (incoming as { thinking?: string }).thinking,
    )
  }
  if (incoming.type === 'tool_use') {
    const curId = (current as { id?: string }).id
    const incId = (incoming as { id?: string }).id
    return !curId || !incId || curId === incId
  }
  return true
}

/**
 * 快照块合并进流式块:text/thinking 一律跳过——字符级 delta 累积是这两个字段的
 * 权威来源(快照会在打字机播完前到达,直接覆盖会瞬间跳满;redacted 模式下快照
 * thinking 还是空串,覆盖会擦掉已积累的明文)。其余字段(signature/input/name/id)
 * 取快照值校正——快照里 tool_use.input 是 CLI 已 parse 的完整对象,比 partial_json
 * 拼接更可靠。
 */
function mergeSnapshotBlock(current: Record<string, unknown>, incoming: ContentBlock) {
  for (const [k, v] of Object.entries(incoming)) {
    if (k === 'text' || k === 'thinking') continue
    current[k] = v
  }
}

// ---- 流结束回调（通知层订阅:完成 toast / 系统通知） ----

type StreamFinishedCallback = (sessionId: string, hasError: boolean) => void
const finishedCallbacks = new Set<StreamFinishedCallback>()

/** 注册流结束回调（useNotifications 用） */
export function onStreamFinished(cb: StreamFinishedCallback) {
  finishedCallbacks.add(cb)
}

/**
 * 「后台结束」脏标记:流式结束时该会话的详情可能没有挂载(在别的 tab/未展开),
 * 下次该会话的详情加载时据此强制 reload,拿到 jsonl 落账后的完整记录。
 */
export const finishedDirty = reactive(new Set<string>())

// ---- 全局事件监听（App 挂载时注册一次,按 session_id 分发） ----

let listenersReady = false

export async function initStreamListeners(): Promise<void> {
  if (listenersReady) return
  listenersReady = true

  await listen<StreamEventPayload>('stream-event', (event) => {
    const payload = event.payload
    const sid = payload.session_id
    if (!sid) return
    const state = getStream(sid)

    switch (payload.kind) {
      case 'block_start':
        // partial messages 模式下,content_block_start 首次出现某 index 的块——
        // 找不到 turn 就新建,然后在 index 位置放置初始块(text:""/tool_use:input{} 等)
        if (payload.message_id && payload.index !== undefined && payload.content_block) {
          let entry = turnIndex.get(payload.message_id)
          if (!entry) {
            const turn: StreamingTurn = { messageId: payload.message_id, content: [] }
            state.streamingTurns.push(turn)
            // reactive 数组里取回代理对象,保证后续 mutate 走响应式
            const reactiveTurn = state.streamingTurns[state.streamingTurns.length - 1]
            entry = { turn: reactiveTurn, sessionId: sid }
            turnIndex.set(payload.message_id, entry)
          }
          ;(entry.turn.content as ContentBlock[])[payload.index] = payload.content_block
          // 新块到达:之前等待执行的工具已经返回(开始下一段输出)
          if (payload.content_block.type === 'tool_use') {
            const name = (payload.content_block as { name?: string }).name ?? i18n.global.t('block.tool')
            state.activeTool = name
          } else {
            state.activeTool = null
          }
          markTailDirty(sid)
          bump()
        }
        break
      case 'block_delta':
        // 字符级增量——text/thinking/signature 走 pending 批处理(避免 markdown 重渲染卡顿);
        // input_json_delta 直接累 toolPartialJson,block_stop 才一次性 parse,无渲染压力
        if (payload.message_id && payload.index !== undefined && payload.delta) {
          const d = payload.delta
          const key = partialJsonKey(payload.message_id, payload.index)
          switch (d.type) {
            case 'text_delta':
              if (d.text) {
                const e = pendingTextDeltas.get(key) ?? {}
                e.text = (e.text ?? '') + d.text
                pendingTextDeltas.set(key, e)
                accumulateTailText(sid, d.text)
              }
              break
            case 'thinking_delta':
              if (d.thinking) {
                const e = pendingTextDeltas.get(key) ?? {}
                e.thinking = (e.thinking ?? '') + d.thinking
                pendingTextDeltas.set(key, e)
              }
              break
            case 'signature_delta':
              if (d.signature) {
                const e = pendingTextDeltas.get(key) ?? {}
                e.signature = d.signature
                pendingTextDeltas.set(key, e)
              }
              break
            case 'input_json_delta':
              if (d.partial_json !== undefined) {
                toolPartialJson.set(key, (toolPartialJson.get(key) ?? '') + d.partial_json)
              }
              break
          }
          bump()
        }
        break
      case 'block_stop':
        // 该 block 不再有 delta。text 残余不在此落地——交给打字机按节奏播完
        // (流结束的 finishStream 有 flush(true) 兜底防丢);立即落地会让块尾瞬间跳满。
        // tool_use 的 partial_json 在 stop 时 parse 写入 block.input
        if (payload.message_id && payload.index !== undefined) {
          const key = partialJsonKey(payload.message_id, payload.index)
          const accumulated = toolPartialJson.get(key)
          const entry = turnIndex.get(payload.message_id)
          const block = entry?.turn.content[payload.index] as ContentBlock | undefined
          if (accumulated !== undefined) {
            if (block?.type === 'tool_use') {
              try {
                ;(block as { input: Record<string, unknown> }).input = JSON.parse(accumulated)
              } catch {
                // 部分 tool_use 可能 partial_json 为空字符串(参数即 {}),parse 失败安全降级保持 {}
              }
            }
            toolPartialJson.delete(key)
          }
          // 工具块结束 = CLI 即将执行该工具:尾部工具行补目标摘要,状态转「等待工具」
          if (block?.type === 'tool_use') {
            const name = (block as { name?: string }).name ?? i18n.global.t('block.tool')
            const target = toolTarget((block as { input?: Record<string, unknown> }).input)
            state.activeTool = target ? `${name} · ${target}` : name
            markTailDirty(sid)
          }
        }
        bump()
        break
      case 'assistant_message':
        // 终态快照兜底校正。CLI 在每个 content block 完成时发一次快照,
        // content 通常只含该块(非全量消息)——绝不能按 index 硬对齐:
        // 快照 [text] 会覆盖 index 0 的 thinking 块,truncate 会砍掉正在
        // 流式的块,被砍块的后续 delta 全部上不了屏。
        // 策略:按类型序列 cursor 匹配,匹配到的块做字段级合并(保留对象引用,
        // BlockThinking 等组件展开 state 不丢);匹配不到才 append(块级兜底)。
        if (payload.message_id && payload.content) {
          const entry = turnIndex.get(payload.message_id)
          if (entry) {
            const existing = entry.turn
            let cursor = 0
            for (const incoming of payload.content) {
              let matched = -1
              for (let j = cursor; j < existing.content.length; j++) {
                const b = existing.content[j] as ContentBlock | undefined
                if (b && snapshotMatches(b, incoming)) {
                  matched = j
                  break
                }
              }
              if (matched < 0) {
                ;(existing.content as ContentBlock[]).push(incoming)
                cursor = existing.content.length
              } else {
                mergeSnapshotBlock(
                  existing.content[matched] as unknown as Record<string, unknown>,
                  incoming,
                )
                cursor = matched + 1
              }
            }
          } else {
            // 没收到过 block_start(理论上不会,防御性)直接建 turn
            state.streamingTurns.push({
              messageId: payload.message_id,
              content: [...payload.content],
            })
            const reactiveTurn = state.streamingTurns[state.streamingTurns.length - 1]
            turnIndex.set(payload.message_id, { turn: reactiveTurn, sessionId: sid })
          }
          bump()
        }
        break
      case 'result':
        // result 到达,流将结束;工具等待态清空
        state.activeTool = null
        if (payload.context_window) state.realContextWindow = payload.context_window
        if (payload.input_tokens) state.realUsedTokens = payload.input_tokens
        markTailDirty(sid)
        break
      case 'error':
        state.streamError = payload.message || i18n.global.t('common.unknownError')
        markTailDirty(sid)
        break
    }
  })

  await listen<{ session_id: string }>('stream-done', (event) => {
    const sid = event.payload?.session_id
    if (sid) finishStream(sid)
  })
}

/** 某一路流结束的统一收尾（其他在播会话不受影响） */
function finishStream(sessionId: string) {
  const state = streams.get(sessionId)
  if (!state || !state.streaming) return
  // 该会话剩余 pending 一次性落地,确保最后几个字符不丢
  flushTextDeltas(true, sessionId)
  streamingTick.value++
  state.streaming = false
  probeFinishFlip(sessionId)
  frameWatchRelease()
  state.activeTool = null
  finishedDirty.add(sessionId)
  markTailDirty(sessionId)
  // 注意:不清 streamingTurns / pendingUserMessage——SessionDetail 在 reload 拿到
  // jsonl 落账后同 batch 清,避免窗口期空白闪烁;无详情挂载的会话等下次发送时重置。
  const hasError = !!state.streamError
  finishedCallbacks.forEach(cb => {
    try {
      cb(sessionId, hasError)
    } catch (_) {
      /* 通知层异常不阻断流收尾 */
    }
  })
}

// ---- 操作 ----

/** 发送消息并开始流式接收。opts 可选,缺省时不向 CLI 附加 --model / --effort */
async function sendMessage(
  sessionId: string,
  cwd: string,
  message: string,
  opts: SendOptions = {},
) {
  const state = getStream(sessionId)
  const wasStreaming = state.streaming

  // 清掉上一轮的 turn 索引、残余 pending 与尾部
  for (const t of state.streamingTurns) dropTurnTransients(t.messageId)
  tailTextAcc.delete(sessionId)

  if (!wasStreaming) {
    state.streaming = true
    frameWatchRetain()
  }
  state.streamError = null
  state.pendingUserMessage = message
  state.streamingTurns = []
  state.startedAt = Date.now()
  state.activeTool = null
  state.tail = []
  state.lastSent = { cwd, message, opts }
  triggerMetaGeneration(sessionId, cwd)

  try {
    await invoke('start_streaming', {
      sessionId,
      cwd,
      message,
      model: opts.model ?? null,
      effort: opts.effort ?? null,
      channel: opts.channel ?? null,
      advisor: opts.advisor ?? false,
      images: opts.images?.length ? opts.images : null,
    })
    state.rcActive = true
  } catch (e) {
    state.streamError = String(e)
    finishStream(sessionId)
  }
}

/** 出错重试:用最近一次发送的消息与参数重发(FR-003 就地决策) */
async function retrySession(sessionId: string): Promise<boolean> {
  const state = streams.get(sessionId)
  if (!state || state.streaming || !state.lastSent) return false
  const { cwd, message, opts } = state.lastSent
  state.streamError = null
  await sendMessage(sessionId, cwd, message, opts)
  return true
}

/** 清空某会话的流式渲染区(不影响磁盘 jsonl)。供 /clear 与会话切换使用 */
function clearStreamingTurns(sessionId: string) {
  const state = streams.get(sessionId)
  if (!state) return
  for (const t of state.streamingTurns) dropTurnTransients(t.messageId)
  state.streamingTurns = []
  state.pendingUserMessage = null
  state.streamError = null
  markTailDirty(sessionId)
}

/** 中断当前回复（发 interrupt 控制消息，不杀进程；
 *  stream-done 由 CLI interrupt 响应的 result 事件驱动，无需前端同步 finishStream。
 *  interrupt 失败时前端兜底收尾，防止永远卡在 streaming 状态） */
async function stopStreaming(sessionId: string) {
  try {
    await invoke('stop_streaming', { sessionId })
  } catch (_) {
    finishStream(sessionId)
  }
}

/** 关闭会话进程（SIGTERM → 5s → SIGKILL） */
async function closeSession(sessionId: string) {
  try {
    await invoke('close_session', { sessionId })
  } catch (_) {
    // ignore
  }
}

async function setPermissionMode(sessionId: string, mode: string) {
  await invoke('set_permission_mode', { sessionId, mode })
}

export function useStreaming() {
  return {
    streams,
    getStream,
    sendMessage,
    retrySession,
    stopStreaming,
    closeSession,
    clearStreamingTurns,
    setPermissionMode,
  }
}
