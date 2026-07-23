import { reactive } from 'vue'
import { listen } from '@tauri-apps/api/event'
import i18n from '../locales'
import { fileName } from '../utils/path'
import { isSessionStreaming } from './useStreaming'
import { maybeNotifySystem } from './useNotifications'

/**
 * 会话状态跟踪扩展（选装）的前端侧：消费 Rust 监听信号文件后 emit 的
 * `turn-signal` 事件，为「外部终端里跑的会话」补上 turn 级状态。
 * 应用内流式会话有第一手协议信号，本模块对其只做去重避让。
 */

export interface TurnSignal {
  sessionId: string
  state: 'started' | 'completed' | 'failed' | 'blocked' | 'ended' | string
  /** unix 秒 */
  ts: number
  cwd: string | null
}

/** session → 最新信号。reactive Map：useSessionStatus 的 computed 依赖它自动重算 */
const signals = reactive(new Map<string, TurnSignal>())

/** started 信号的陈旧兜底：CLI crash 不会发 Stop，超时视为已结束 */
const STALE_RUNNING_SECS = 2 * 3600
/** 只有新鲜信号才弹系统通知，启动重放的历史信号静默入表 */
const NOTIFY_FRESH_SECS = 60

let listenerReady = false

/** App 挂载时注册一次（扩展未安装时无事件到达，零开销） */
export async function initTurnSignalListener(): Promise<void> {
  if (listenerReady) return
  listenerReady = true
  await listen<TurnSignal>('turn-signal', (event) => {
    const sig = event.payload
    if (!sig?.sessionId) return
    signals.set(sig.sessionId, sig)
    void maybeNotifyExternal(sig)
  })
}

/**
 * 外部会话的完成/阻塞/失败提醒。
 * 应用内会话（正在流式）走自己的通知闭环，这里避让；
 * 前台抑制与授权逻辑复用 maybeNotifySystem。
 */
async function maybeNotifyExternal(sig: TurnSignal): Promise<void> {
  if (isSessionStreaming(sig.sessionId)) return
  if (Math.abs(Date.now() / 1000 - sig.ts) > NOTIFY_FRESH_SECS) return

  const t = i18n.global.t
  const project = sig.cwd ? fileName(sig.cwd) : ''
  const title = project ? `Claude Code · ${project}` : 'Claude Code'
  if (sig.state === 'completed') {
    await maybeNotifySystem(title, t('settings.turnSignal.notifyDone'))
  } else if (sig.state === 'blocked') {
    await maybeNotifySystem(title, t('settings.turnSignal.notifyBlocked'))
  } else if (sig.state === 'failed') {
    await maybeNotifySystem(title, t('settings.turnSignal.notifyFailed'))
  }
}

/**
 * 查某会话的有效外部信号；started 超过陈旧窗口视为无信号。
 * 供 useSessionStatus 在无应用内流状态时兜底派生。
 */
export function signalFor(sessionId: string | null): TurnSignal | null {
  if (!sessionId) return null
  const sig = signals.get(sessionId)
  if (!sig) return null
  if (sig.state === 'started' && Date.now() / 1000 - sig.ts > STALE_RUNNING_SECS) {
    return null
  }
  return sig
}
