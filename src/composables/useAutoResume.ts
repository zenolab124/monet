/**
 * 额度恢复后自动续跑：按会话开启，收到 5 小时窗口的被拒事件后，在恢复时刻发送一次「继续」。
 *
 * 规则在 lib/autoResumePolicy；这里负责持久化（~/.monet/auto-resume.json）、定时、
 * 取消与发送。只在 Monet 主进程运行时执行；系统睡眠后定时器迟到，按过期规则处理。
 */

import { reactive, computed, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import i18n from '../locales'
import { getStream, onBeforeSend, useStreaming, type SendOptions } from './useStreaming'
import { onRateLimit, pushNotice, type RateLimitState } from './useSessionSignals'
import {
  fireDecision,
  isRejectedAfterResume,
  persistableOpts,
  resumeDueAt,
  type AutoResumePlan,
} from '@/lib/autoResumePolicy'

interface PersistedState {
  enabled: string[]
  plans: AutoResumePlan[]
}

const state = reactive({
  enabled: new Set<string>(),
  plans: new Map<string, AutoResumePlan>(),
})
const timers = new Map<string, ReturnType<typeof setTimeout>>()
let lastSendAt: number | null = null
let sendingOwn = false
let ready = false

function persist() {
  const payload: PersistedState = {
    enabled: [...state.enabled],
    plans: [...state.plans.values()],
  }
  void invoke('save_auto_resume_state', { content: JSON.stringify(payload) }).catch(() => {
    // 持久化失败不影响本次运行内的调度
  })
}

function clearTimer(sessionId: string) {
  const timer = timers.get(sessionId)
  if (timer) clearTimeout(timer)
  timers.delete(sessionId)
}

function removePlan(sessionId: string) {
  clearTimer(sessionId)
  if (state.plans.delete(sessionId)) persist()
}

function schedule(sessionId: string, at: number, startup = false) {
  clearTimer(sessionId)
  const delay = Math.max(0, at - Date.now())
  timers.set(sessionId, setTimeout(() => fire(sessionId, startup), delay))
}

async function fire(sessionId: string, startup = false) {
  timers.delete(sessionId)
  const plan = state.plans.get(sessionId)
  if (!plan || plan.state !== 'scheduled') return
  if (!state.enabled.has(sessionId)) {
    removePlan(sessionId)
    return
  }
  const decision = fireDecision(plan, Date.now(), lastSendAt, startup)
  if (decision.action === 'expire') {
    removePlan(sessionId)
    pushNotice(sessionId, 'auto_resume_expired')
    return
  }
  if (decision.action === 'wait') {
    schedule(sessionId, decision.until)
    return
  }
  // 用户已在该会话里发了新消息（正在运行）：视为手动接管
  if (getStream(sessionId).streaming) {
    removePlan(sessionId)
    return
  }
  lastSendAt = Date.now()
  plan.state = 'sent'
  plan.sentAt = lastSendAt
  persist()
  sendingOwn = true
  try {
    const { sendMessage } = useStreaming()
    await sendMessage(
      sessionId,
      plan.cwd,
      i18n.global.t('session.signals.continueMessage'),
      plan.opts as SendOptions,
      false,
    )
    pushNotice(sessionId, 'auto_resume_sent')
  } finally {
    sendingOwn = false
  }
}

function handleRateLimit(sessionId: string, event: RateLimitState) {
  const existing = state.plans.get(sessionId)
  if (isRejectedAfterResume(existing, event)) {
    removePlan(sessionId)
    pushNotice(sessionId, 'auto_resume_still_limited')
    return
  }
  if (existing?.state === 'sent' && event.status !== 'rejected') {
    // 续跑后额度已恢复，计划完成
    removePlan(sessionId)
    return
  }
  if (!state.enabled.has(sessionId) || existing?.state === 'scheduled') return
  planFromEvent(sessionId, event)
}

function planFromEvent(sessionId: string, event: RateLimitState): boolean {
  const dueAt = resumeDueAt(event)
  const lastSent = getStream(sessionId).lastSent
  if (dueAt === null || !lastSent) return false
  state.plans.set(sessionId, {
    sessionId,
    cwd: lastSent.cwd,
    opts: persistableOpts(lastSent.opts as Record<string, unknown>),
    dueAt,
    rejectedAt: event.receivedAt,
    state: 'scheduled',
  })
  persist()
  schedule(sessionId, dueAt)
  return true
}

export async function initAutoResume(): Promise<void> {
  if (ready) return
  ready = true
  try {
    const raw = await invoke<string | null>('load_auto_resume_state')
    const parsed = raw ? (JSON.parse(raw) as Partial<PersistedState>) : null
    for (const id of parsed?.enabled ?? []) state.enabled.add(id)
    for (const plan of parsed?.plans ?? []) {
      if (plan?.sessionId && plan.state === 'scheduled') {
        state.plans.set(plan.sessionId, plan)
        // 启动补检查：已过点的计划延后 30 秒执行或按过期规则作废
        schedule(plan.sessionId, Math.max(plan.dueAt, Date.now()), true)
      }
    }
  } catch {
    // 文件缺失或损坏：从空状态开始
  }
  onRateLimit(handleRateLimit)
  onBeforeSend((sessionId) => {
    if (sendingOwn) return
    const plan = state.plans.get(sessionId)
    if (plan?.state === 'scheduled') removePlan(sessionId)
  })
}

/** 会话的续跑开关与计划 */
export function useAutoResume(sessionId: Ref<string | null | undefined>) {
  const enabled = computed(() => !!sessionId.value && state.enabled.has(sessionId.value))
  const plan = computed(() => (sessionId.value ? state.plans.get(sessionId.value) ?? null : null))

  function setEnabled(value: boolean, current: RateLimitState | null) {
    const id = sessionId.value
    if (!id) return
    if (value) {
      state.enabled.add(id)
      // 已处于被拒状态时立即按当前事件安排
      if (current && !state.plans.has(id)) planFromEvent(id, current)
    } else {
      state.enabled.delete(id)
      clearTimer(id)
      state.plans.delete(id)
    }
    persist()
  }

  return { enabled, plan, setEnabled }
}
