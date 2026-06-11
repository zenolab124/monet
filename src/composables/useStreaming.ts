import { reactive, computed, ref, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ContentBlock } from '@/types'
import type { EffortLevel } from './useSessionSettings'

export interface SendOptions {
  model?: string
  effort?: EffortLevel
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
 * 自适应打字间隔(ms):queue 越长间隔越短,保证 burst 后 UI 不拖延;queue 短则慢节奏,
 * 接近人阅读速度。每次只吐 1 字(由 flushTextDeltas 控制),速度完全由本函数的间隔决定。
 */
function typingInterval(queueLen: number): number {
  if (queueLen <= 3) return 80    // ~12 字/秒,慢节奏(接近正常朗读)
  if (queueLen <= 15) return 40   // ~25 字/秒,中节奏
  if (queueLen <= 50) return 16   // ~60 字/秒,RAF 级,追赶 burst
  return 5                        // ~200 字/秒,巨长队列加速排空
}

function totalPendingChars(): number {
  let total = 0
  pendingTextDeltas.forEach(p => {
    total += (p.text?.length ?? 0) + (p.thinking?.length ?? 0)
  })
  return total
}

/**
 * @param allAtOnce    true:一次性 flush 全部(block_stop / 流结束用,确保不丢字符)
 *                     false:每个 entry 吐 1 字符(平滑打字机播放)
 * @param onlySession  仅 flush 指定会话的 pending(某一路流结束时不影响其他在播会话)
 * @returns 是否还有未消化字符
 */
function flushTextDeltas(allAtOnce = false, onlySession?: string): boolean {
  if (pendingTextDeltas.size === 0) return false
  let hasRemaining = false
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

    if (p.text !== undefined && block.type === 'text') {
      const n = allAtOnce ? p.text.length : 1
      ;(block as { text: string }).text += p.text.slice(0, n)
      const rem = p.text.slice(n)
      if (rem) {
        p.text = rem
        hasRemaining = true
      } else {
        p.text = undefined
      }
    }
    if (p.thinking !== undefined && block.type === 'thinking') {
      const n = allAtOnce ? p.thinking.length : 1
      ;(block as { thinking: string }).thinking += p.thinking.slice(0, n)
      const rem = p.thinking.slice(n)
      if (rem) {
        p.thinking = rem
        hasRemaining = true
      } else {
        p.thinking = undefined
      }
    }
    if (p.signature !== undefined && block.type === 'thinking') {
      ;(block as Record<string, unknown>).signature = p.signature
      p.signature = undefined
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
            const name = (payload.content_block as { name?: string }).name ?? '工具'
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
        // block_stop:该 block 不会再有 delta,剩余 pending 必须全部落地,不分块
        flushTextDeltas(true)
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
            const name = (block as { name?: string }).name ?? '工具'
            const target = toolTarget((block as { input?: Record<string, unknown> }).input)
            state.activeTool = target ? `${name} · ${target}` : name
            markTailDirty(sid)
          }
        }
        bump()
        break
      case 'assistant_message':
        // 终态快照,作为字符级增量的兜底校正——按 index 对齐 mutate
        // 保留对象引用(BlockThinking 等组件展开 state 不丢)
        if (payload.message_id && payload.content) {
          const entry = turnIndex.get(payload.message_id)
          if (entry) {
            const existing = entry.turn
            const newContent = payload.content
            for (let i = 0; i < newContent.length; i++) {
              const incoming = newContent[i]
              const current = existing.content[i] as ContentBlock | undefined
              if (current && current.type === incoming.type) {
                Object.assign(current as Record<string, unknown>, incoming)
              } else {
                ;(existing.content as ContentBlock[])[i] = incoming
              }
            }
            if (existing.content.length > newContent.length) {
              existing.content.length = newContent.length
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
        markTailDirty(sid)
        break
      case 'error':
        state.streamError = payload.message || '未知错误'
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
  if (state.streaming) return

  // 清掉上一轮的 turn 索引与尾部
  for (const t of state.streamingTurns) turnIndex.delete(t.messageId)
  tailTextAcc.delete(sessionId)

  state.streaming = true
  state.streamError = null
  state.pendingUserMessage = message
  state.streamingTurns = []
  state.startedAt = Date.now()
  state.activeTool = null
  state.tail = []
  state.lastSent = { cwd, message, opts }

  try {
    await invoke('start_streaming', {
      sessionId,
      cwd,
      message,
      model: opts.model ?? null,
      effort: opts.effort ?? null,
    })
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
  for (const t of state.streamingTurns) turnIndex.delete(t.messageId)
  state.streamingTurns = []
  state.pendingUserMessage = null
  state.streamError = null
  markTailDirty(sessionId)
}

/** 中断某会话的流式 */
async function stopStreaming(sessionId: string) {
  try {
    await invoke('stop_streaming', { sessionId })
  } catch (_) {
    // ignore
  }
  finishStream(sessionId)
}

export function useStreaming() {
  return {
    streams,
    getStream,
    sendMessage,
    retrySession,
    stopStreaming,
    clearStreamingTurns,
  }
}
