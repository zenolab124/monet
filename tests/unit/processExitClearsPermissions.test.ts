import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'

const listeners = new Map<string, (event: { payload: unknown }) => void>()
const invoke = vi.fn()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return () => listeners.delete(name)
  }),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

function emit(name: string, payload: unknown) {
  const handler = listeners.get(name)
  if (!handler) throw new Error(`listener ${name} not registered`)
  handler({ payload })
}

describe('process exit clears pending permission requests', () => {
  beforeEach(() => {
    vi.resetModules()
    listeners.clear()
    invoke.mockReset()
    invoke.mockResolvedValue(undefined)
    vi.stubGlobal('document', { addEventListener: vi.fn(), visibilityState: 'visible', hidden: false })
  })

  it('drops the exited session\'s questions and answers them with a deny', async () => {
    const permissions = await import('../../src/composables/usePermissionRequests')
    const streaming = await import('../../src/composables/useStreaming')
    await permissions.initPermissionListener()
    await streaming.initStreamListeners()

    emit('permission-request', {
      requestId: 'req-exited',
      sessionId: 'session-a',
      toolName: 'AskUserQuestion',
      input: { questions: [] },
      timestamp: 1,
    })
    emit('permission-request', {
      requestId: 'req-other',
      sessionId: 'session-b',
      toolName: 'AskUserQuestion',
      input: { questions: [] },
      timestamp: 2,
    })

    emit('session-process-exited', { session_id: 'session-a' })
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith('respond_permission', expect.objectContaining({
      requestId: 'req-exited',
      allow: false,
    })))

    expect(permissions.queueForSession(ref('session-a')).value).toEqual([])
    expect(permissions.queueForSession(ref('session-b')).value.map(r => r.requestId)).toEqual(['req-other'])
  })
})
