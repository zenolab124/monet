import { reactive, computed, ref, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ContentBlock } from '@/types'
import { triggerMetaGeneration } from './useSessionMeta'
import { useHtmlVisual, HTML_VISUAL_PROMPT } from '@/features'
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
  permissionMode?: string
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
export interface PendingQueueItem {
  message: string
  opts: SendOptions
}

export interface SessionStreamState {
  streaming: boolean
  streamingTurns: StreamingTurn[]
  pendingUserMessage: string | null
  /** 发送时附带的图片（流式待发/发送中回显用） */
  pendingImages: SendOptions['images'] | null
  /** BTW 预输入队列：流式中用户发的消息暂存于此，前一轮 result 落账后再逐条发送 */
  pendingQueue: PendingQueueItem[]
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
  /** API 报告的 output token 数（result 事件 modelUsage.outputTokens） */
  realOutputTokens: number | null
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
    pendingImages: null,
    pendingQueue: [],
    streamError: null,
    startedAt: null,
    activeTool: null,
    tail: [],
    lastSent: null,
    realContextWindow: null,
    realUsedTokens: null,
    realOutputTokens: null,
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
// thinking 块计时：block_start 记时刻，block_stop 算差值
const thinkingStartTimes = new Map<string, number>()

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
  for (const key of thinkingStartTimes.keys()) {
    if (key.startsWith(prefix)) thinkingStartTimes.delete(key)
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
 * 自适应打字间隔(ms)。积压浅时保留打字机质感,深时快速追赶。
 */
function typingInterval(queueLen: number): number {
  if (queueLen <= 5) return 55     // ~18 字/秒,舒适打字节奏
  if (queueLen <= 40) return 30    // ~33 字/秒,略快
  if (queueLen <= 200) return 20   // 配合 charsPerTick 加速
  return 16                        // 深积压:贴合 rAF 节奏全速追赶
}

/** 每帧吐字数:积压越深每帧多吐,2s 内消化大块快照 */
function charsPerTick(queueLen: number): number {
  if (queueLen <= 100) return 1
  if (queueLen <= 500) return 3
  if (queueLen <= 2000) return 10
  return 30
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
 *                     false:打字机节奏;thinking/signature 到帧即全量落地
 * @param onlySession  仅 flush 指定会话的 pending(某一路流结束时不影响其他在播会话)
 * @returns 是否还有未消化字符
 */
function flushTextDeltas(allAtOnce = false, onlySession?: string): boolean {
  if (pendingTextDeltas.size === 0) return false
  const pending = totalPendingChars()
  if (pending > 5000) allAtOnce = true
  let hasRemaining = false
  const take = allAtOnce ? Infinity : charsPerTick(pending)

  // 统计每个会话有几个待播 text 块,有后续块时前面的瞬间吐完
  const sessionTextKeys = new Map<string, string[]>()
  if (!allAtOnce) {
    pendingTextDeltas.forEach((p, key) => {
      if (p.text === undefined) return
      const mid = key.slice(0, key.indexOf('#'))
      const entry = turnIndex.get(mid)
      if (!entry) return
      if (onlySession && entry.sessionId !== onlySession) return
      const arr = sessionTextKeys.get(entry.sessionId)
      if (arr) arr.push(key)
      else sessionTextKeys.set(entry.sessionId, [key])
    })
  }
  const instantKeys = new Set<string>()
  sessionTextKeys.forEach(keys => {
    for (let i = 0; i < keys.length - 1; i++) instantKeys.add(keys[i])
  })

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

    if (p.thinking !== undefined && block.type === 'thinking') {
      ;(block as { thinking: string }).thinking += p.thinking
      p.thinking = undefined
    }
    if (p.signature !== undefined && block.type === 'thinking') {
      ;(block as Record<string, unknown>).signature = p.signature
      p.signature = undefined
    }

    if (p.text !== undefined && block.type === 'text') {
      const instant = instantKeys.has(key)
      const actualTake = instant ? Infinity : take
      ;(block as { text: string }).text += p.text.slice(0, actualTake)
      const rem = p.text.slice(actualTake)
      if (rem) {
        p.text = rem
        hasRemaining = true
      } else {
        p.text = undefined
      }
    }

    if (p.text === undefined && p.thinking === undefined && p.signature === undefined) {
      pendingTextDeltas.delete(key)
    }
  })
  return hasRemaining
}

/** 正在排空打字机的会话(CLI 已 done,等打字机播完再翻 streaming=false) */
const drainingStreams = new Set<string>()

function hasSessionPending(sessionId: string): boolean {
  for (const [key] of pendingTextDeltas) {
    const mid = key.slice(0, key.indexOf('#'))
    const entry = turnIndex.get(mid)
    if (entry?.sessionId === sessionId) return true
  }
  return false
}

function bump() {
  if (rafId !== null) return
  const delay = typingInterval(totalPendingChars())
  rafId = window.setTimeout(() => {
    rafId = null
    const hasRemaining = flushTextDeltas(false)
    streamingTick.value++
    // 检查排空中的会话:打字机播完 → 完成收尾
    if (drainingStreams.size > 0) {
      for (const sid of [...drainingStreams]) {
        if (!hasSessionPending(sid)) {
          drainingStreams.delete(sid)
          completeFinish(sid)
        }
      }
    }
    if (hasRemaining || drainingStreams.size > 0) bump()
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
 * 冲掉指定消息中 < beforeIndex 的所有未播文字,让它们立刻显示完整。
 * 场景:新块到达时,前面还在打字的块不该阻塞后续内容。
 */
function flushPendingBefore(messageId: string, beforeIndex: number) {
  const prefix = `${messageId}#`
  for (const [key, p] of pendingTextDeltas) {
    if (!key.startsWith(prefix)) continue
    const idx = parseInt(key.slice(prefix.length), 10)
    if (idx >= beforeIndex) continue
    const entry = turnIndex.get(messageId)
    if (!entry) continue
    const block = entry.turn.content[idx]
    if (!block) continue
    if (p.text !== undefined && block.type === 'text') {
      ;(block as { text: string }).text += p.text
      p.text = undefined
    }
    if (p.text === undefined && p.thinking === undefined && p.signature === undefined) {
      pendingTextDeltas.delete(key)
    }
  }
}

/**
 * 从 assistant 快照提取增量。
 * text → 喂入打字机队列(逐字播放)。
 * thinking → 直接设值(不走打字机,折叠态逐字无意义)。
 */
function feedSnapshotText(
  messageId: string,
  index: number,
  incoming: ContentBlock,
  stripped: Record<string, unknown>,
  sessionId: string,
) {
  if (incoming.type === 'text') {
    const key = partialJsonKey(messageId, index)
    const incText = (incoming as { text?: string }).text ?? ''
    if (!incText) return
    const curText = typeof stripped.text === 'string' ? stripped.text : ''
    const pendingLen = pendingTextDeltas.get(key)?.text?.length ?? 0
    const alreadyKnown = curText.length + pendingLen
    if (incText.length > alreadyKnown) {
      const delta = incText.slice(alreadyKnown)
      const e = pendingTextDeltas.get(key) ?? {}
      e.text = (e.text ?? '') + delta
      pendingTextDeltas.set(key, e)
      accumulateTailText(sessionId, delta)
    }
    stripped.text = curText
  } else if (incoming.type === 'thinking') {
    // thinking 直接设值——折叠态逐字无意义,且会阻塞后续 text 块
    stripped.thinking = (incoming as { thinking?: string }).thinking ?? ''
  }
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
          // thinking 块计时起点
          if (payload.content_block.type === 'thinking') {
            thinkingStartTimes.set(partialJsonKey(payload.message_id, payload.index), Date.now())
          }
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
          // thinking 块计时终点
          if (block?.type === 'thinking') {
            const startTime = thinkingStartTimes.get(key)
            if (startTime) {
              ;(block as { _thinkingMs?: number })._thinkingMs = Date.now() - startTime
              thinkingStartTimes.delete(key)
            }
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
        // 快照校正 + 打字机喂料。CLI 2.1.177+ 不再发 content_block_delta,
        // 只发 assistant 快照(可能多次:空→部分→完整)。检测文本增量,
        // 扣除已播+已排队部分,喂入 pendingTextDeltas 复用打字机管线。
        // 兼容旧 CLI:若 block_delta 已喂过相同字符,增量为 0,不重复。
        if (payload.message_id && payload.content) {
          const mid = payload.message_id
          const entry = turnIndex.get(mid)
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
                const idx = existing.content.length
                // 新块到达 → 冲掉前面还在打字的块,避免积压阻塞
                flushPendingBefore(mid, idx)
                const stripped: Record<string, unknown> = { ...incoming }
                if (incoming.type === 'text') stripped.text = ''
                feedSnapshotText(mid, idx, incoming, stripped, sid)
                ;(existing.content as ContentBlock[]).push(stripped as ContentBlock)
                cursor = existing.content.length
              } else {
                feedSnapshotText(mid, matched, incoming, existing.content[matched] as unknown as Record<string, unknown>, sid)
                mergeSnapshotBlock(
                  existing.content[matched] as unknown as Record<string, unknown>,
                  incoming,
                )
                cursor = matched + 1
              }
            }
          } else {
            const strippedContent: ContentBlock[] = payload.content.map(b => {
              const s: Record<string, unknown> = { ...b }
              if (b.type === 'text') s.text = ''
              // thinking 保留全量(直接显示,不走打字机)
              return s as ContentBlock
            })
            state.streamingTurns.push({
              messageId: mid,
              content: strippedContent,
            })
            const reactiveTurn = state.streamingTurns[state.streamingTurns.length - 1]
            turnIndex.set(mid, { turn: reactiveTurn, sessionId: sid })
            for (let idx = 0; idx < payload.content.length; idx++) {
              feedSnapshotText(mid, idx, payload.content[idx], strippedContent[idx] as unknown as Record<string, unknown>, sid)
            }
          }
          bump()
        }
        break
      case 'result':
        // result 到达,流将结束;工具等待态清空
        state.activeTool = null
        if (payload.context_window) state.realContextWindow = payload.context_window
        if (payload.input_tokens) state.realUsedTokens = payload.input_tokens
        if (payload.output_tokens) state.realOutputTokens = payload.output_tokens
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

/**
 * 某一路流结束(CLI done)——如果打字机还有未播内容,进入排空模式,
 * 等打字机自然播完再翻 streaming=false,避免:
 *   1. flushTextDeltas(true) 一次性倾倒剩余文字(文字跳变)
 *   2. streaming=false 触发 BlockText plain→shiki 切换(布局抖动)
 *   3. 300ms 后 records reload(DOM 重建)
 * 三次突变叠在一帧 → 可见闪烁。
 * 通知回调(toast/系统通知)立即触发,不等打字机。
 */
function finishStream(sessionId: string) {
  const state = streams.get(sessionId)
  if (!state || !state.streaming) return
  console.log(`%c ========== [stream] finishStream sid=${sessionId.slice(0, 8)} pending=${hasSessionPending(sessionId)} t=${performance.now().toFixed(0)} ==========`, 'color:#f59e0b;font-weight:bold')
  state.activeTool = null
  markTailDirty(sessionId)
  const hasError = !!state.streamError
  finishedCallbacks.forEach(cb => {
    try { cb(sessionId, hasError) } catch (_) { /* */ }
  })
  if (hasSessionPending(sessionId)) {
    drainingStreams.add(sessionId)
    bump()
    return
  }
  completeFinish(sessionId)
}

// ====== 排除法调试开关 ======
const SKIP_STREAMING_FALSE = false  // true = 不翻 streaming=false

function completeFinish(sessionId: string) {
  const state = streams.get(sessionId)
  if (!state || !state.streaming) return
  console.log(`%c ========== [stream] completeFinish → streaming=false sid=${sessionId.slice(0, 8)} t=${performance.now().toFixed(0)} ==========`, 'color:#ef4444;font-weight:bold')
  if (SKIP_STREAMING_FALSE) {
    console.log('%c ========== [stream] SKIP_STREAMING_FALSE — 不翻 streaming ==========', 'color:#ef4444;font-weight:bold')
    return
  }
  state.streaming = false
  probeFinishFlip(sessionId)
  frameWatchRelease()
  finishedDirty.add(sessionId)
  markTailDirty(sessionId)
  streamingTick.value++
  // 注意:不清 streamingTurns / pendingUserMessage——SessionDetail 在 reload 拿到
  // jsonl 落账后同 batch 清,避免窗口期空白闪烁;无详情挂载的会话等下次发送时重置。
  if (state.lastSent) {
    triggerMetaGeneration(sessionId, state.lastSent.cwd)
  }
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

  if (state.streaming) {
    state.pendingQueue.push({ message, opts })
    return
  }

  // 清掉上一轮的 turn 索引、残余 pending 与尾部
  for (const t of state.streamingTurns) dropTurnTransients(t.messageId)
  tailTextAcc.delete(sessionId)

  state.streaming = true
  frameWatchRetain()
  state.streamError = null
  state.pendingUserMessage = message
  state.pendingImages = opts.images?.length ? opts.images : null
  state.streamingTurns = []
  state.startedAt = Date.now()
  state.activeTool = null
  state.tail = []
  state.lastSent = { cwd, message, opts }
  const { enabled: htmlVisualEnabled } = useHtmlVisual()
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
      permissionMode: opts.permissionMode ?? null,
      appendSystemPrompt: htmlVisualEnabled.value ? HTML_VISUAL_PROMPT : null,
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
  state.pendingImages = null
  state.streamError = null
  markTailDirty(sessionId)
}

function removePendingQueueItem(sessionId: string, index: number) {
  const state = streams.get(sessionId)
  if (state) state.pendingQueue.splice(index, 1)
}

/** 消费 BTW 队列队首：前一轮 reload 落账后由 SessionDetail 调用 */
async function consumePendingQueue(sessionId: string, cwd: string) {
  const state = streams.get(sessionId)
  if (!state || state.streaming || state.pendingQueue.length === 0) return
  const next = state.pendingQueue.shift()!
  await sendMessage(sessionId, cwd, next.message, next.opts)
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

export function useStreaming() {
  return {
    streams,
    getStream,
    sendMessage,
    retrySession,
    stopStreaming,
    closeSession,
    clearStreamingTurns,
    removePendingQueueItem,
    consumePendingQueue,
  }
}
