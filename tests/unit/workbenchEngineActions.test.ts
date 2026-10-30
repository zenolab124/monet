import { describe, expect, it } from 'vitest'
import { resolveWorkbenchEngineActions } from '../../src/engines/workbenchActions'
import type { EngineCapabilities } from '../../src/engines/types'

function runtime(overrides: Partial<NonNullable<EngineCapabilities['runtime']>>) {
  return {
    create: false,
    resume: true,
    fork: false,
    forkWithCwd: false,
    sendWhileRunning: false,
    interrupt: true,
    streaming: { text: 'delta', reasoning: 'delta', toolProgress: 'delta' },
    modelCatalog: 'dynamic',
    interactions: [],
    ...overrides,
  } satisfies NonNullable<EngineCapabilities['runtime']>
}

describe('workbench engine actions', () => {
  it('keeps native session actions available when the descriptor is unknown', () => {
    expect(resolveWorkbenchEngineActions(undefined, true)).toEqual({
      create: true,
      fork: true,
      race: true,
    })
  })

  it('offers no actions for a native instance that declares no runtime', () => {
    expect(resolveWorkbenchEngineActions(null, true)).toEqual({
      create: false,
      fork: false,
      race: false,
    })
  })

  it('enables Codex-style create, fork, and race capabilities together', () => {
    expect(resolveWorkbenchEngineActions(runtime({ create: true, fork: true }), false)).toEqual({
      create: true,
      fork: true,
      race: true,
    })
  })

  it('does not advertise race when an engine cannot fork', () => {
    expect(resolveWorkbenchEngineActions(runtime({ create: true, fork: false }), false)).toEqual({
      create: true,
      fork: false,
      race: false,
    })
  })
})
