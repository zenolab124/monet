import { describe, expect, it } from 'vitest'
import { buildEngineChoices, type EngineChoiceState } from '../../src/engines/engineChoices'
import { instanceKey } from '../../src/engines/identity'
import type { EngineDescriptor, EngineHealth, EngineInstanceId } from '../../src/engines/types'

const local: EngineInstanceId = { engineId: 'claude-code', instanceId: 'default' }
const second: EngineInstanceId = { engineId: 'claude-code', instanceId: 'second-a' }
const codex: EngineInstanceId = { engineId: 'codex', instanceId: 'default' }

function descriptor(instance: EngineInstanceId, displayName: string, create = true): EngineDescriptor {
  return {
    instance,
    displayName,
    enabled: true,
    capabilities: { runtime: { create } } as unknown as EngineDescriptor['capabilities'],
    ui: { identity: 'native', sessionSurface: 'native', installGuideUrl: null },
  }
}

function runtimeHealth(instance: EngineInstanceId, available: boolean): EngineHealth {
  return { instance, runtime: { available, reasonCode: null } } as EngineHealth
}

function state(overrides: Partial<EngineChoiceState>): EngineChoiceState {
  return { engines: [], health: {}, errors: {}, loading: false, ...overrides }
}

describe('buildEngineChoices', () => {
  it('produces the same two choices as before when each engine has one instance', () => {
    const choices = buildEngineChoices(state({
      engines: [descriptor(codex, 'Codex'), descriptor(local, 'Claude Code')],
      health: {
        [instanceKey(local)]: runtimeHealth(local, true),
        [instanceKey(codex)]: runtimeHealth(codex, true),
      },
    }))

    expect(choices.map(choice => [choice.engineId, choice.label, choice.available])).toEqual([
      ['claude-code', 'Claude Code', true],
      ['codex', 'Codex', true],
    ])
    expect(choices[0].id).toBe(instanceKey(local))
  })

  it('lists every instance of an engine as its own choice keyed by instance', () => {
    const choices = buildEngineChoices(state({
      engines: [descriptor(local, 'Claude Code'), descriptor(second, 'Claude Code · second-a')],
      health: {
        [instanceKey(local)]: runtimeHealth(local, true),
        [instanceKey(second)]: runtimeHealth(second, false),
      },
    }))
    const claude = choices.filter(choice => choice.engineId === 'claude-code')

    expect(claude.map(choice => choice.id)).toEqual([instanceKey(local), instanceKey(second)])
    expect(claude.map(choice => choice.available)).toEqual([true, false])
    expect(claude[1].label).toBe('Claude Code · second-a')
    expect(claude[1].descriptor?.instance).toEqual(second)
  })

  it('keeps an unavailable placeholder for an engine that has not registered', () => {
    const [claude, codexChoice] = buildEngineChoices(state({
      engines: [descriptor(local, 'Claude Code')],
      loading: true,
    }))

    expect(claude.checking).toBe(true)
    expect(codexChoice).toMatchObject({ id: 'codex', descriptor: null, available: false, checking: true })
  })

  it('stops checking once health failed and requires runtime creation support', () => {
    const [claude] = buildEngineChoices(state({
      engines: [descriptor(local, 'Claude Code', false)],
      health: { [instanceKey(local)]: runtimeHealth(local, true) },
    }))
    const [failed] = buildEngineChoices(state({
      engines: [descriptor(local, 'Claude Code')],
      errors: { [instanceKey(local)]: 'boom' },
    }))

    expect(claude.available).toBe(false)
    expect(failed).toMatchObject({ available: false, checking: false })
  })
})
