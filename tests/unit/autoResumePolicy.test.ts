import { describe, expect, it } from 'vitest'
import {
  AUTO_RESUME_EXPIRE_MS,
  AUTO_RESUME_STAGGER_MS,
  fireDecision,
  isRejectedAfterResume,
  persistableOpts,
  resumeDueAt,
  type AutoResumePlan,
} from '../../src/lib/autoResumePolicy'

const reset = 1_761_750_000_000

function plan(overrides: Partial<AutoResumePlan> = {}): AutoResumePlan {
  return {
    sessionId: 's1',
    cwd: '/workspace/app',
    opts: {},
    dueAt: reset + 90_000,
    rejectedAt: reset - 3_600_000,
    state: 'scheduled',
    ...overrides,
  }
}

describe('auto resume policy', () => {
  it('schedules five-hour rejections between 60 and 120 seconds after reset', () => {
    const event = { status: 'rejected', rateLimitType: 'five_hour', resetsAtMs: reset, receivedAt: reset - 1 }
    expect(resumeDueAt(event, () => 0)).toBe(reset + 60_000)
    expect(resumeDueAt(event, () => 0.999)).toBeLessThan(reset + 120_000)
  })

  it('ignores other windows, warnings and events without reset time', () => {
    expect(resumeDueAt({ status: 'rejected', rateLimitType: 'seven_day', resetsAtMs: reset, receivedAt: 0 })).toBeNull()
    expect(resumeDueAt({ status: 'allowed_warning', rateLimitType: 'five_hour', resetsAtMs: reset, receivedAt: 0 })).toBeNull()
    expect(resumeDueAt({ status: 'rejected', rateLimitType: 'five_hour', resetsAtMs: null, receivedAt: 0 })).toBeNull()
  })

  it('waits until due, sends when due and staggers consecutive sends', () => {
    const p = plan()
    expect(fireDecision(p, p.dueAt - 1, null)).toEqual({ action: 'wait', until: p.dueAt })
    expect(fireDecision(p, p.dueAt, null)).toEqual({ action: 'send' })
    expect(fireDecision(p, p.dueAt, p.dueAt - 10_000))
      .toEqual({ action: 'wait', until: p.dueAt - 10_000 + AUTO_RESUME_STAGGER_MS })
  })

  it('catches up within two hours after restart and expires beyond that', () => {
    const p = plan()
    expect(fireDecision(p, p.dueAt + 3_600_000, null, true).action).toBe('wait')
    expect(fireDecision(p, p.dueAt + AUTO_RESUME_EXPIRE_MS + 1, null, true)).toEqual({ action: 'expire' })
    expect(fireDecision(p, p.dueAt + AUTO_RESUME_EXPIRE_MS + 1, null)).toEqual({ action: 'expire' })
  })

  it('stops after a rejection that follows the resume message', () => {
    const sent = plan({ state: 'sent', sentAt: reset + 100_000 })
    expect(isRejectedAfterResume(sent, { status: 'rejected', rateLimitType: 'five_hour', resetsAtMs: reset, receivedAt: reset + 120_000 })).toBe(true)
    expect(isRejectedAfterResume(sent, { status: 'rejected', rateLimitType: 'five_hour', resetsAtMs: reset, receivedAt: reset })).toBe(false)
    expect(isRejectedAfterResume(plan(), { status: 'rejected', rateLimitType: 'five_hour', resetsAtMs: reset, receivedAt: reset + 120_000 })).toBe(false)
  })

  it('drops images and one-shot flags from persisted options', () => {
    expect(persistableOpts({ model: 'opus', images: [1], forceNew: true, forkSource: 'x', channel: null }))
      .toEqual({ model: 'opus', channel: null })
  })
})
