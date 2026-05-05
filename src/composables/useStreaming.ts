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

interface StreamEventPayload {
  kind: 'assistant_message' | 'result' | 'error'
  message_id?: string
  content?: ContentBlock[]
  text?: string
  cost_usd?: number
  message?: string
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
      case 'assistant_message':
        if (payload.message_id && payload.content) {
          const existing = streamingTurns.value.find(t => t.messageId === payload.message_id)
          if (existing) {
            // claude CLI 在 --print 模式下,stream-json 每个 progress 事件都重发完整 content 快照(非 delta)。
            // 按 index 对齐 merge:同 index 同 type → Object.assign 原对象(保留组件实例,
            // BlockThinking 等组件的展开 state 不丢);type 变了或新增 → 直接赋值/追加。
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
            // 截断:新快照短于旧数组时移除末尾(理论上不会,防御性)
            if (existing.content.length > newContent.length) {
              existing.content.length = newContent.length
            }
          } else {
            streamingTurns.value.push({
              messageId: payload.message_id,
              content: [...payload.content],
            })
          }
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
  pendingUserMessage.value = null
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
