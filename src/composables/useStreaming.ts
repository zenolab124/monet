import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
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

const streaming = ref(false)
const streamingTurns = ref<StreamingTurn[]>([])
const pendingUserMessage = ref<string | null>(null)
const streamError = ref<string | null>(null)

let unlistenEvent: UnlistenFn | null = null
let unlistenDone: UnlistenFn | null = null

interface BlockDeltaPayload {
  type: 'text_delta' | 'input_json_delta' | 'thinking_delta' | 'signature_delta' | string
  text?: string
  partial_json?: string
  thinking?: string
  signature?: string
}

interface StreamEventPayload {
  kind: 'assistant_message' | 'block_start' | 'block_delta' | 'block_stop' | 'result' | 'error'
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

// 累积每个 (messageId, index) 上 tool_use 的 partial_json 字符串,
// block_stop 时 JSON.parse 一次性写入 block.input
const toolPartialJson = new Map<string, string>()
const partialJsonKey = (messageId: string, index: number) => `${messageId}#${index}`

// RAF 批 flush:字符级 delta 高频到达,直接 mutate 会让 BlockText 的 markdown-it+shiki
// 重渲染 100+ 次/秒,导致卡顿且 SessionDetail 的滚动跟随 watcher 无法识别字符级变化。
// 策略:text/thinking/signature delta 先累到 pendingTextDeltas,每帧合并 flush 一次到 block。
// streamingTick 每帧递增一次,SessionDetail watch 它即可统一处理滚动跟随。
// input_json_delta 不走此路径——tool_use input 不经 markdown,无渲染压力,且 block_stop 才 parse。
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
 * 接近人阅读速度。每次只吐 1 字(由 flushTextDeltas 控制),速度完全由本函数的间隔决定,
 * 节奏感比"变 chunkSize"更稳——不会一帧蹦多字下一帧蹦一字。
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
 * @param allAtOnce  true:一次性 flush 全部(block_stop / cleanup 用,确保不丢字符)
 *                   false:每个 entry 吐 1 字符(平滑打字机播放)
 * @returns 是否还有未消化字符
 */
function flushTextDeltas(allAtOnce = false): boolean {
  if (pendingTextDeltas.size === 0) return false
  let hasRemaining = false
  pendingTextDeltas.forEach((p, key) => {
    const hashIdx = key.indexOf('#')
    if (hashIdx < 0) return
    const mid = key.slice(0, hashIdx)
    const index = parseInt(key.slice(hashIdx + 1), 10)
    const turn = streamingTurns.value.find(t => t.messageId === mid)
    if (!turn) return
    const block = turn.content[index] as ContentBlock | undefined
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

/** 发送消息并开始流式接收。opts 可选,缺省时不向 CLI 附加 --model / --effort */
async function sendMessage(
  sessionId: string,
  cwd: string,
  message: string,
  opts: SendOptions = {},
) {
  if (streaming.value) return

  streaming.value = true
  streamError.value = null
  pendingUserMessage.value = message
  streamingTurns.value = []

  // 注册事件监听
  unlistenEvent = await listen<StreamEventPayload>('stream-event', (event) => {
    const payload = event.payload
    switch (payload.kind) {
      case 'block_start':
        // partial messages 模式下,content_block_start 首次出现某 index 的块——
        // 找不到 turn 就新建,然后在 index 位置放置初始块(text:""/tool_use:input{} 等)
        if (payload.message_id && payload.index !== undefined && payload.content_block) {
          let turn = streamingTurns.value.find(t => t.messageId === payload.message_id)
          if (!turn) {
            turn = { messageId: payload.message_id, content: [] }
            streamingTurns.value.push(turn)
          }
          // 同 messageId 重发 block_start 不应该发生,但出现就替换(保留对象引用语义按 index)
          ;(turn.content as ContentBlock[])[payload.index] = payload.content_block
          bump()
        }
        break
      case 'block_delta':
        // 字符级增量——text/thinking/signature 走 pending RAF 批处理(避免 markdown 重渲染卡顿);
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
          if (accumulated !== undefined) {
            const turn = streamingTurns.value.find(t => t.messageId === payload.message_id)
            const block = turn?.content[payload.index] as ContentBlock | undefined
            if (block?.type === 'tool_use') {
              try {
                ;(block as { input: Record<string, unknown> }).input = JSON.parse(accumulated)
              } catch {
                // 部分 tool_use 可能 partial_json 为空字符串(参数即 {}),parse 失败安全降级保持 {}
              }
            }
            toolPartialJson.delete(key)
          }
        }
        bump()
        break
      case 'assistant_message':
        // 终态快照,作为字符级增量的兜底校正——按 index 对齐 mutate
        // 保留对象引用(BlockThinking 等组件展开 state 不丢)
        if (payload.message_id && payload.content) {
          const existing = streamingTurns.value.find(t => t.messageId === payload.message_id)
          if (existing) {
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
            streamingTurns.value.push({
              messageId: payload.message_id,
              content: [...payload.content],
            })
          }
          bump()
        }
        break
      case 'result':
        // result 到达，流将结束
        break
      case 'error':
        streamError.value = payload.message || '未知错误'
        break
    }
  })

  unlistenDone = await listen('stream-done', () => {
    cleanup()
  })

  try {
    await invoke('start_streaming', {
      sessionId,
      cwd,
      message,
      model: opts.model ?? null,
      effort: opts.effort ?? null,
    })
  } catch (e) {
    streamError.value = String(e)
    cleanup()
  }
}

/** 清空当前的流式渲染区(不影响磁盘 jsonl)。供 /clear 命令使用 */
function clearStreamingTurns() {
  streamingTurns.value = []
  pendingUserMessage.value = null
  streamError.value = null
}

/** 中断流式 */
async function stopStreaming() {
  try {
    await invoke('stop_streaming')
  } catch (_) {
    // ignore
  }
  cleanup()
}

function cleanup() {
  streaming.value = false
  // 注意:不在此处清 pendingUserMessage——它显示的是用户最近发出的消息,
  // 在流式刚结束、reload 尚未拿到 jsonl 的窗口期内,user bubble 必须保留可见,
  // 否则会造成"completed 出现的瞬间用户消息消失、整段 assistant 内容上移"的视觉闪烁。
  // 清空交给 SessionDetail watch streaming 内 reload 完成后的 clearStreamingTurns 同步执行,
  // 这样 pendingUserMessage 消失与历史区新 user record 出现位于同一 reactive batch,无闪。
  // 流式被中断/结束前一次性 flush 全部,确保最后几个字符的 pending delta 也落到 block
  flushTextDeltas(true)
  if (rafId !== null) {
    clearTimeout(rafId)
    rafId = null
  }
  // 中断后清空 pending,避免下次流式启动时残留拼接错乱
  pendingTextDeltas.clear()
  toolPartialJson.clear()
  // 不清 streamingTurns:让 SessionDetail 在 reload 完成后再清,避免渲染断片
  // (流式结束 → cleanup 立即清 → reload 异步等 jsonl flush 完成 → 中间窗口期详情页空白)
  unlistenEvent?.()
  unlistenDone?.()
  unlistenEvent = null
  unlistenDone = null
}

export function useStreaming() {
  return {
    streaming,
    streamingTurns,
    pendingUserMessage,
    streamError,
    sendMessage,
    stopStreaming,
    clearStreamingTurns,
  }
}
