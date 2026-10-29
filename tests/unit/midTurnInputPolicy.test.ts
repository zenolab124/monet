import { describe, expect, it } from 'vitest'
import {
  canSendToCurrentTurn,
  cliSupportsMidTurnInput,
  injectionTransition,
  sameRunConfig,
  type MidTurnContext,
} from '../../src/lib/midTurnInputPolicy'

function context(overrides: Partial<MidTurnContext> = {}): MidTurnContext {
  return {
    ownProcessBusy: true,
    externalRunning: false,
    processAlive: true,
    cliVersion: '2.1.306',
    activeConfig: { model: 'opus', effort: 'high', channel: null },
    ...overrides,
  }
}

describe('midTurnInputPolicy', () => {
  it('allows a text item with the running configuration', () => {
    expect(canSendToCurrentTurn(context(), { model: 'opus', effort: 'high' })).toBe(true)
  })

  it('treats missing and empty settings as the same default', () => {
    expect(sameRunConfig({ channel: null, extraArgs: '' }, {})).toBe(true)
    expect(sameRunConfig({ advisor: false }, { advisor: undefined })).toBe(true)
    expect(sameRunConfig({ model: 'opus' }, { model: 'sonnet' })).toBe(false)
  })

  it('falls back to interrupting when the configuration differs', () => {
    expect(canSendToCurrentTurn(context(), { model: 'sonnet', effort: 'high' })).toBe(false)
    expect(canSendToCurrentTurn(context(), { model: 'opus', effort: 'high', permissionMode: 'plan' })).toBe(false)
  })

  it('never inserts images, forced restarts or into idle or external sessions', () => {
    const item = { model: 'opus', effort: 'high' }
    expect(canSendToCurrentTurn(context(), { ...item, images: [{}] })).toBe(false)
    expect(canSendToCurrentTurn(context(), { ...item, forceNew: true })).toBe(false)
    expect(canSendToCurrentTurn(context({ ownProcessBusy: false }), item)).toBe(false)
    expect(canSendToCurrentTurn(context({ externalRunning: true }), item)).toBe(false)
    expect(canSendToCurrentTurn(context({ processAlive: false }), item)).toBe(false)
    expect(canSendToCurrentTurn(context({ activeConfig: null }), item)).toBe(false)
  })

  it('requires a CLI that reports command lifecycle', () => {
    expect(cliSupportsMidTurnInput('2.1.306')).toBe(true)
    expect(cliSupportsMidTurnInput('2.1.310')).toBe(true)
    expect(cliSupportsMidTurnInput('2.2.0')).toBe(true)
    expect(cliSupportsMidTurnInput('2.1.305')).toBe(false)
    expect(cliSupportsMidTurnInput('1.9.999')).toBe(false)
    expect(cliSupportsMidTurnInput(null)).toBe(false)
    expect(cliSupportsMidTurnInput('dev')).toBe(false)
  })

  it('folds an accepted insertion into the turn and removes it once it ends', () => {
    expect(injectionTransition('sending', 'queued')).toEqual({ kind: 'set', injection: 'queued' })
    expect(injectionTransition('queued', 'started')).toEqual({ kind: 'set', injection: 'started' })
    expect(injectionTransition('started', 'completed')).toEqual({ kind: 'remove' })
    expect(injectionTransition('started', 'cancelled')).toEqual({ kind: 'remove' })
    expect(injectionTransition('queued', 'completed')).toEqual({ kind: 'remove' })
  })

  it('returns unsent insertions to the queue without retrying them', () => {
    expect(injectionTransition('sending', 'refused')).toEqual({ kind: 'fail' })
    expect(injectionTransition('queued', 'cancelled')).toEqual({ kind: 'fail' })
    expect(injectionTransition('queued', 'discarded')).toEqual({ kind: 'fail' })
    expect(injectionTransition('sending', 'timeout')).toEqual({ kind: 'fail', injection: 'timed_out' })
  })

  it('claims late reports after a timeout', () => {
    expect(injectionTransition('timed_out', 'queued')).toEqual({ kind: 'ignore' })
    expect(injectionTransition('timed_out', 'started')).toEqual({ kind: 'remove' })
    expect(injectionTransition('timed_out', 'completed')).toEqual({ kind: 'remove' })
    expect(injectionTransition('timed_out', 'cancelled')).toEqual({ kind: 'clear' })
    expect(injectionTransition(undefined, 'started')).toEqual({ kind: 'ignore' })
  })
})
