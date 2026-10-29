import { beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import type { EngineInstanceId } from '../../src/engines/types'

const listeners = new Map<string, (event: { payload: unknown }) => void>()
const listModels = vi.fn()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return () => listeners.delete(name)
  }),
}))
vi.mock('@/engines/client', () => ({ listModels }))

const local: EngineInstanceId = { engineId: 'claude-code', instanceId: 'default' }
const second: EngineInstanceId = { engineId: 'claude-code', instanceId: 'second-a' }

async function freshCatalog() {
  vi.resetModules()
  return import('../../src/composables/useClaudeModelCatalog')
}

describe('Claude model catalog per instance', () => {
  beforeEach(() => {
    listeners.clear()
    listModels.mockReset()
    listModels.mockResolvedValue([])
  })

  it('loads each instance from its own engine catalog', async () => {
    const catalog = await freshCatalog()
    listModels.mockImplementation(async (instance: EngineInstanceId) => instance.instanceId === 'second-a'
      ? [{ model: 'claude-second-only', displayName: 'Second Only' }]
      : [{ model: 'claude-local-only', displayName: 'Local Only' }])

    await catalog.loadClaudeModelCatalog(local)
    await catalog.loadClaudeModelCatalog(second)
    await catalog.loadClaudeModelCatalog(second)

    expect(listModels).toHaveBeenCalledTimes(2)
    expect(listModels).toHaveBeenCalledWith(local)
    expect(listModels).toHaveBeenCalledWith(second)
    expect(catalog.currentKnownClaudeModelIds(second).has('claude-second-only')).toBe(true)
    expect(catalog.currentKnownClaudeModelIds(second).has('claude-local-only')).toBe(false)
    expect(catalog.currentKnownClaudeModelIds().has('claude-local-only')).toBe(true)
  })

  it('routes handshake catalogs by the reporting instance and treats unlabeled ones as local', async () => {
    const catalog = await freshCatalog()
    await catalog.loadClaudeModelCatalog(local)
    const emit = listeners.get('session-catalog')!

    emit({ payload: { engine: second, catalog: { models: [{ value: 'claude-second-x', display_name: 'X' }] } } })
    emit({ payload: { catalog: { models: [{ value: 'claude-local-y', display_name: 'Y' }] } } })

    expect(catalog.currentKnownClaudeModelIds(second).has('claude-second-x')).toBe(true)
    expect(catalog.currentKnownClaudeModelIds(second).has('claude-local-y')).toBe(false)
    expect(catalog.currentKnownClaudeModelIds(local).has('claude-local-y')).toBe(true)
  })

  it('follows the instance a composable is bound to', async () => {
    const catalog = await freshCatalog()
    catalog.setReportedClaudeModels([{ value: 'claude-opus-local', display_name: 'Local Opus' }], local)
    catalog.setReportedClaudeModels([{ value: 'claude-opus-second', display_name: 'Second Opus' }], second)

    const scope = effectScope()
    const bound = { value: local as EngineInstanceId }
    const { pinnedModels } = scope.run(() => catalog.useClaudeModelCatalog(() => bound.value))!
    expect(pinnedModels.value.map(model => model.id)).toContain('claude-opus-local')

    const { pinnedModels: secondModels } = scope.run(() => catalog.useClaudeModelCatalog(second))!
    await nextTick()
    expect(secondModels.value.map(model => model.id)).toContain('claude-opus-second')
    expect(secondModels.value.map(model => model.id)).not.toContain('claude-opus-local')
    scope.stop()
  })
})
