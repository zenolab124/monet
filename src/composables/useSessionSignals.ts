/**
 * 会话级实时信号：引擎报出的额度、实时状态、能力目录与会话内提示。
 *
 * 与 useStreaming 的消息流状态分离——这些信号不进入时间线记录、不落盘，
 * 只服务于会话详情的提示区、工作台卡片与额度面板。Rust 侧只发中立事件，
 * 本模块不按引擎名分支。
 */

import { reactive, computed, type Ref } from 'vue'
import { listen } from '@tauri-apps/api/event'

export type RateLimitStatus = 'allowed' | 'allowed_warning' | 'rejected'

export interface RateLimitState {
  status: RateLimitStatus
  rateLimitType: string | null
  resetsAtMs: number | null
  utilization: number | null
  isUsingOverage: boolean
  receivedAt: number
}

export type LiveStatus =
  | { kind: 'retrying'; attempt: number | null; maxRetries: number | null }
  | { kind: 'compacting' }

export interface ReportedCommand {
  name: string
  description: string
  argument_hint: string | null
}

export interface ReportedCatalog {
  commands: ReportedCommand[]
  agents: string[]
  models: Array<{ value: string; display_name: string; description: string | null }>
  output_style: string | null
  available_output_styles: string[]
  cli_version: string | null
}

export type SessionNoticeKind =
  | 'permission_mode_failed'
  | 'unsupported_request'
  | 'init_issues'
  | 'model_fallback'
  | 'permission_denied'
  | 'auto_resume_expired'
  | 'auto_resume_still_limited'
  | 'auto_resume_sent'

export interface SessionNotice {
  id: string
  kind: SessionNoticeKind
  params: Record<string, unknown>
  at: number
}

export interface SessionSignalState {
  notices: SessionNotice[]
  rateLimit: RateLimitState | null
  liveStatus: LiveStatus | null
  catalog: ReportedCatalog | null
  /** 权限切换被 CLI 拒绝或超时：界面需回退到的进程真实模式（seq 递增触发 watch） */
  permissionRevert: { mode: string | null; seq: number } | null
  /** 当前轮次序号；同一轮内的不支持请求合并为一条提示 */
  turnSeq: number
}

const signals = reactive(new Map<string, SessionSignalState>())
let noticeCounter = 0

function createState(): SessionSignalState {
  return {
    notices: [],
    rateLimit: null,
    liveStatus: null,
    catalog: null,
    permissionRevert: null,
    turnSeq: 0,
  }
}

export function getSignals(sessionId: string): SessionSignalState {
  let state = signals.get(sessionId)
  if (!state) {
    signals.set(sessionId, createState())
    state = signals.get(sessionId)!
  }
  return state
}

export function useSessionSignals(sessionId: Ref<string | null | undefined>) {
  return computed(() => (sessionId.value ? getSignals(sessionId.value) : null))
}

export function pushNotice(sessionId: string, kind: SessionNoticeKind, params: Record<string, unknown> = {}) {
  const state = getSignals(sessionId)
  noticeCounter += 1
  state.notices.push({ id: `notice-${noticeCounter}`, kind, params, at: Date.now() })
  // 只保留最近 20 条，避免长会话无限累积
  if (state.notices.length > 20) state.notices.splice(0, state.notices.length - 20)
}

export function dismissNotice(sessionId: string, noticeId: string) {
  const state = signals.get(sessionId)
  if (!state) return
  state.notices = state.notices.filter(notice => notice.id !== noticeId)
}

/** 最近一次实时额度事件（任一会话）；额度面板据此决定是否暂停轮询 */
export function latestRateLimit(): (RateLimitState & { sessionId: string }) | null {
  let latest: (RateLimitState & { sessionId: string }) | null = null
  for (const [sessionId, state] of signals) {
    if (state.rateLimit && (!latest || state.rateLimit.receivedAt > latest.receivedAt)) {
      latest = { ...state.rateLimit, sessionId }
    }
  }
  return latest
}

type RateLimitListener = (sessionId: string, state: RateLimitState) => void
const rateLimitListeners = new Set<RateLimitListener>()

/** 订阅额度事件（自动续跑调度用） */
export function onRateLimit(listener: RateLimitListener): () => void {
  rateLimitListeners.add(listener)
  return () => rateLimitListeners.delete(listener)
}

function recordUnsupported(sessionId: string, subtype: string) {
  const state = getSignals(sessionId)
  const last = state.notices[state.notices.length - 1]
  if (last?.kind === 'unsupported_request' && last.params.turnSeq === state.turnSeq) {
    const counts = { ...(last.params.counts as Record<string, number>) }
    counts[subtype] = (counts[subtype] ?? 0) + 1
    last.params = { ...last.params, counts }
    return
  }
  pushNotice(sessionId, 'unsupported_request', { turnSeq: state.turnSeq, counts: { [subtype]: 1 } })
}

let listenersReady = false

export async function initSessionSignalListeners(): Promise<void> {
  if (listenersReady) return
  listenersReady = true

  await Promise.all([
    listen<Record<string, unknown> & { session_id: string; kind: string }>('session-notice', (event) => {
      const { session_id: sessionId, kind, ...rest } = event.payload
      if (!sessionId) return
      if (kind === 'unsupported_request') {
        recordUnsupported(sessionId, String(rest.subtype ?? 'unknown'))
      } else if (kind === 'init_issues' || kind === 'model_fallback' || kind === 'permission_denied') {
        pushNotice(sessionId, kind, rest)
      }
    }),

    listen<{ session_id: string; kind: string; attempt?: number | null; max_retries?: number | null }>(
      'session-live-status',
      (event) => {
        const { session_id: sessionId, kind } = event.payload
        if (!sessionId) return
        const state = getSignals(sessionId)
        if (kind === 'retrying') {
          state.liveStatus = {
            kind: 'retrying',
            attempt: event.payload.attempt ?? null,
            maxRetries: event.payload.max_retries ?? null,
          }
        } else if (kind === 'compacting') {
          state.liveStatus = { kind: 'compacting' }
        } else if (kind === 'compact_done' && state.liveStatus?.kind === 'compacting') {
          state.liveStatus = null
        }
      },
    ),

    listen<{
      session_id: string
      status: RateLimitStatus
      rate_limit_type: string | null
      resets_at_ms: number | null
      utilization: number | null
      is_using_overage: boolean
    }>('session-rate-limit', (event) => {
      const payload = event.payload
      if (!payload.session_id) return
      const state = getSignals(payload.session_id)
      state.rateLimit = {
        status: payload.status,
        rateLimitType: payload.rate_limit_type,
        resetsAtMs: payload.resets_at_ms,
        utilization: payload.utilization,
        isUsingOverage: payload.is_using_overage,
        receivedAt: Date.now(),
      }
      for (const listener of rateLimitListeners) listener(payload.session_id, state.rateLimit)
    }),

    listen<{ session_id: string; catalog: ReportedCatalog }>('session-catalog', (event) => {
      if (!event.payload.session_id || !event.payload.catalog) return
      getSignals(event.payload.session_id).catalog = event.payload.catalog
    }),

    listen<{ session_id: string; ok: boolean; mode: string | null; requested: string; error: string | null }>(
      'permission-mode-result',
      (event) => {
        const payload = event.payload
        if (!payload.session_id) return
        const state = getSignals(payload.session_id)
        const seq = (state.permissionRevert?.seq ?? 0) + 1
        if (payload.ok) {
          // 迟到的成功判决（超时后才到）也要把界面同步到真实模式
          state.permissionRevert = { mode: payload.mode, seq }
          return
        }
        state.permissionRevert = { mode: payload.mode, seq }
        pushNotice(payload.session_id, 'permission_mode_failed', {
          requested: payload.requested,
          error: payload.error,
        })
      },
    ),

    // 轮次结束：合并窗口切换；实时状态在轮次边界一律清除
    listen<{ session_id: string }>('stream-done', (event) => {
      const sessionId = event.payload?.session_id
      if (!sessionId) return
      const state = signals.get(sessionId)
      if (!state) return
      state.turnSeq += 1
      state.liveStatus = null
    }),

    // 重试中的状态在下一条正常输出到达后消失
    listen<Array<{ session_id: string }>>('stream-events', (event) => {
      for (const payload of event.payload) {
        const state = signals.get(payload.session_id)
        if (state?.liveStatus?.kind === 'retrying') state.liveStatus = null
      }
    }),
  ])
}
