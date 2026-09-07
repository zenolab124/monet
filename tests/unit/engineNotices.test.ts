import { beforeEach, describe, expect, it, vi } from 'vitest'

const { invoke, listen } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen }))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

beforeEach(() => {
  vi.resetModules()
  invoke.mockReset()
  listen.mockReset().mockImplementation(() => new Promise(() => {}))
})

describe('Codex settings refresh', () => {
  it('fetches a new snapshot after an in-flight check when settings were saved', async () => {
    const oldCheck = deferred<unknown>()
    const newCheck = deferred<unknown>()
    invoke.mockReturnValueOnce(oldCheck.promise).mockReturnValueOnce(newCheck.promise)
    const { useEngineNotices } = await import('@/composables/useEngineNotices')
    const state = useEngineNotices()
    const savedRefresh = state.refreshEngineNotices(true)
    oldCheck.resolve({ binaryPath: '/opt/old/codex', runtimeRestartRequired: false })
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(2))
    newCheck.resolve({ binaryPath: '/opt/new/codex', runtimeRestartRequired: true })
    await savedRefresh
    expect(state.codexInfo.value?.binaryPath).toBe('/opt/new/codex')
    expect(state.codexInfo.value?.runtimeRestartRequired).toBe(true)
    expect(state.checking.value).toBe(false)
  })

  it('shares ordinary concurrent checks', async () => {
    const pending = deferred<unknown>()
    invoke.mockReturnValueOnce(pending.promise)
    const { useEngineNotices } = await import('@/composables/useEngineNotices')
    const state = useEngineNotices()
    const first = state.refreshEngineNotices()
    const second = state.refreshEngineNotices()
    pending.resolve({ binaryPath: '/opt/codex', runtimeRestartRequired: false })
    await Promise.all([first, second])
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(state.codexInfo.value?.binaryPath).toBe('/opt/codex')
  })
})
