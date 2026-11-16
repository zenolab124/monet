import { beforeEach, describe, expect, it, vi } from 'vitest'
import { isReactive } from 'vue'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))

describe('rewind session state', () => {
  beforeEach(() => {
    vi.resetModules()
    invoke.mockReset()
    invoke.mockResolvedValue(undefined)
    vi.stubGlobal('document', { addEventListener: vi.fn(), visibilityState: 'visible', hidden: false })
  })

  it('blocks sends during rewind and clears only the restored session after success', async () => {
    const { useStreaming } = await import('../../src/composables/useStreaming')
    const api = useStreaming()
    await Promise.resolve()
    invoke.mockClear()
    vi.stubGlobal('window', { setTimeout: vi.fn(() => 1) })
    const state = api.getStream('restored')
    expect(isReactive(state)).toBe(true)
    state.rewinding = true
    state.pendingUserMessage = 'old prompt'
    state.lastSent = { cwd: '/workspace/example', message: 'old prompt', opts: {} }
    state.realUsedTokens = 120
    api.enqueuePendingQueueItem('restored', '/workspace/example', 'queued prompt', {})
    api.enqueuePendingQueueItem('other', '/workspace/example', 'keep this prompt', {})

    expect(await api.sendMessage('restored', '/workspace/example', 'new prompt', {})).toBe(false)
    await api.consumePendingQueue('restored', '/workspace/example')
    expect(invoke).not.toHaveBeenCalled()
    expect(state.pendingQueue).toHaveLength(1)

    api.resetAfterRewind('restored')
    expect(state.pendingQueue).toEqual([])
    expect(state.pendingUserMessage).toBeNull()
    expect(state.lastSent).toBeNull()
    expect(state.realUsedTokens).toBeNull()
    expect(state.rewinding).toBe(true)
    expect(api.getStream('other').pendingQueue).toHaveLength(1)
    expect(await api.retrySession('restored')).toBe(false)
  })

  it('preserves an uncertain outcome across reload and pauses automatic queue consumption', async () => {
    const { useStreaming } = await import('../../src/composables/useStreaming')
    const api = useStreaming()
    await Promise.resolve()
    invoke.mockClear()
    vi.stubGlobal('window', { setTimeout: vi.fn(() => 1) })
    const state = api.getStream('uncertain')
    state.streamError = 'REWIND_OUTCOME_UNKNOWN'
    api.enqueuePendingQueueItem('uncertain', '/workspace/example', 'queued prompt', {})
    api.clearStreamingTurns('uncertain')
    await api.consumePendingQueue('uncertain', '/workspace/example')
    expect(state.streamError).toBe('REWIND_OUTCOME_UNKNOWN')
    expect(state.pendingQueue).toHaveLength(1)
    expect(invoke).not.toHaveBeenCalled()
  })
})
