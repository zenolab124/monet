/**
 * 额度恢复后自动续跑的纯规则（调度与持久化见 useAutoResume）。
 *
 * 约束：只对 5 小时窗口生效；每次被拒最多发送一条消息；错过时刻 2 小时内补发、
 * 超过则作废；多个会话相邻两次发送至少间隔 30 秒；不轮询额度接口。
 */

export const AUTO_RESUME_WINDOW = 'five_hour'
export const AUTO_RESUME_MIN_DELAY_MS = 60_000
export const AUTO_RESUME_JITTER_MS = 60_000
export const AUTO_RESUME_EXPIRE_MS = 2 * 60 * 60 * 1000
export const AUTO_RESUME_STAGGER_MS = 30_000
export const AUTO_RESUME_STARTUP_DELAY_MS = 30_000

export interface AutoResumePlan {
  sessionId: string
  cwd: string
  /** 发送时沿用的会话配置（不含图片） */
  opts: Record<string, unknown>
  dueAt: number
  rejectedAt: number
  state: 'scheduled' | 'sent'
  sentAt?: number
}

export interface RejectionInput {
  status: string
  rateLimitType: string | null
  resetsAtMs: number | null
  receivedAt: number
}

/** 被拒事件是否可安排续跑；可以则返回预定时刻 */
export function resumeDueAt(event: RejectionInput, random: () => number = Math.random): number | null {
  if (event.status !== 'rejected') return null
  if (event.rateLimitType !== AUTO_RESUME_WINDOW) return null
  if (!event.resetsAtMs || !Number.isFinite(event.resetsAtMs)) return null
  const jitter = Math.floor(random() * AUTO_RESUME_JITTER_MS)
  return event.resetsAtMs + AUTO_RESUME_MIN_DELAY_MS + jitter
}

export type FireDecision =
  | { action: 'send' }
  | { action: 'wait'; until: number }
  | { action: 'expire' }

/** 到点（或启动补检查）时的处置 */
export function fireDecision(
  plan: AutoResumePlan,
  now: number,
  lastSendAt: number | null,
  startup = false,
): FireDecision {
  if (now - plan.dueAt > AUTO_RESUME_EXPIRE_MS) return { action: 'expire' }
  if (now < plan.dueAt) return { action: 'wait', until: plan.dueAt }
  if (startup) return { action: 'wait', until: now + AUTO_RESUME_STARTUP_DELAY_MS }
  if (lastSendAt !== null && now - lastSendAt < AUTO_RESUME_STAGGER_MS) {
    return { action: 'wait', until: lastSendAt + AUTO_RESUME_STAGGER_MS }
  }
  return { action: 'send' }
}

/** 续跑发出后再次被拒：停止，不再安排 */
export function isRejectedAfterResume(plan: AutoResumePlan | undefined, event: RejectionInput): boolean {
  return !!plan && plan.state === 'sent' && event.status === 'rejected'
    && plan.sentAt !== undefined && event.receivedAt >= plan.sentAt
}

/** 剔除不可持久化或不应沿用的发送参数 */
export function persistableOpts(opts: Record<string, unknown>): Record<string, unknown> {
  const { images: _images, forceNew: _forceNew, forkSource: _forkSource, ...rest } = opts
  return rest
}
