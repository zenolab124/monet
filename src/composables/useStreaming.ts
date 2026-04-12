import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ContentBlock } from '@/types'

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

/** 发送消息并开始流式接收 */
async function sendMessage(sessionId: string, cwd: string, message: string) {
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
          // 合并或替换同 messageId 的 turn
          const existing = streamingTurns.value.find(t => t.messageId === payload.message_id)
          if (existing) {
            existing.content = payload.content
          } else {
            streamingTurns.value.push({
              messageId: payload.message_id,
              content: payload.content,
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
    await invoke('start_streaming', { sessionId, cwd, message })
  } catch (e) {
    streamError.value = String(e)
    cleanup()
  }
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
  }
}
