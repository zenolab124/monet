import { reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface HintState {
  text: string | null
  loading: boolean
}

const hints = reactive<Map<string, HintState>>(new Map())

const cache = new Map<string, string>()

function cacheKey(toolName: string, input: Record<string, unknown>): string {
  const pick = (k: string): string =>
    typeof input[k] === 'string' ? (input[k] as string) : ''
  const sig = pick('command') || pick('file_path') || pick('notebook_path') || pick('url') || ''
  return `${toolName}::${sig.slice(0, 200)}`
}

export function requestHint(requestId: string, toolName: string, input: Record<string, unknown>) {
  if (hints.has(requestId)) return

  const key = cacheKey(toolName, input)
  const cached = cache.get(key)
  if (cached) {
    hints.set(requestId, { text: cached, loading: false })
    return
  }

  hints.set(requestId, { text: null, loading: true })

  const inputJson = JSON.stringify(input, null, 2)
  invoke<string>('generate_permission_hint', { toolName, inputJson })
    .then((text) => {
      cache.set(key, text)
      hints.set(requestId, { text, loading: false })
    })
    .catch(() => {
      hints.set(requestId, { text: null, loading: false })
    })
}

export function getHint(requestId: string): HintState | undefined {
  return hints.get(requestId)
}

export function clearHint(requestId: string) {
  hints.delete(requestId)
}
