/**
 * 队列项能否直接插入正在运行的 Claude 轮次（不中断）的纯规则。
 *
 * 插入只能沿用当前进程的启动配置：模型、努力等级、渠道等任一不同都需要重启进程，
 * 只能走"中断并发送"。带图片的项也不插入——插入消息落盘为 attachment，
 * 图片协议按 message.content 定位，无法回显。
 */

/** 已核对 command_lifecycle 回报的 CLI 基线；更早的版本无法确认送达，不提供插入 */
export const MID_TURN_INPUT_MIN_CLI = '2.1.306'

export interface RunConfig {
  model?: string
  effort?: string | null
  fastMode?: boolean
  channel?: string | null
  advisor?: boolean
  chrome?: boolean
  extraArgs?: string
  permissionMode?: string
  forceNew?: boolean
  images?: unknown[]
}

export interface MidTurnContext {
  /** 本窗口自持进程正在跑一轮 */
  ownProcessBusy: boolean
  /** 会话由外部进程（终端等）占用 */
  externalRunning: boolean
  processAlive: boolean
  cliVersion: string | null
  /** 当前进程最近一次发送使用的配置 */
  activeConfig: RunConfig | null
}

const CONFIG_KEYS = ['model', 'effort', 'fastMode', 'channel', 'advisor', 'chrome', 'extraArgs', 'permissionMode'] as const

function normalized(config: RunConfig, key: typeof CONFIG_KEYS[number]): unknown {
  const value = config[key]
  if (value === undefined || value === null || value === '') return key === 'advisor' || key === 'chrome' ? false : null
  return value
}

export function sameRunConfig(left: RunConfig, right: RunConfig): boolean {
  return CONFIG_KEYS.every(key => normalized(left, key) === normalized(right, key))
}

export function cliSupportsMidTurnInput(version: string | null): boolean {
  if (!version) return false
  const parse = (raw: string) => raw.split(/[.-]/).slice(0, 3).map(part => Number.parseInt(part, 10))
  const actual = parse(version)
  const minimum = parse(MID_TURN_INPUT_MIN_CLI)
  if (actual.some(Number.isNaN)) return false
  for (let i = 0; i < minimum.length; i += 1) {
    if ((actual[i] ?? 0) !== minimum[i]) return (actual[i] ?? 0) > minimum[i]
  }
  return true
}

export function canSendToCurrentTurn(context: MidTurnContext, item: RunConfig): boolean {
  return context.ownProcessBusy
    && !context.externalRunning
    && context.processAlive
    && cliSupportsMidTurnInput(context.cliVersion)
    && context.activeConfig !== null
    && !item.forceNew
    && !item.images?.length
    && sameRunConfig(item, context.activeConfig)
}

/** 插入当前轮次的进度：sending 等待 CLI 回报，queued 已入 CLI 队列，
 *  started 已并入轮次，timed_out 确认超时（已撤回，迟到的 started 仍会认领） */
export type QueueInjectionState = 'sending' | 'queued' | 'started' | 'timed_out'

export type InjectionTransition =
  | { kind: 'remove' }
  | { kind: 'set'; injection: QueueInjectionState }
  | { kind: 'fail'; injection?: QueueInjectionState }
  | { kind: 'clear' }
  | { kind: 'ignore' }

/**
 * CLI 回报（command_lifecycle 状态或宿主的 timeout）对队列项的作用。
 * started 之后消息已进入对话，任何终态都只需移出队列；未运行就失败的退回原位置。
 */
export function injectionTransition(current: QueueInjectionState | undefined, event: string): InjectionTransition {
  if (!current) return { kind: 'ignore' }
  if (event === 'queued') return current === 'sending' ? { kind: 'set', injection: 'queued' } : { kind: 'ignore' }
  if (event === 'started') return current === 'timed_out' ? { kind: 'remove' } : { kind: 'set', injection: 'started' }
  if (event === 'completed' || current === 'started') return { kind: 'remove' }
  if (current === 'timed_out') return { kind: 'clear' }
  return event === 'timeout' ? { kind: 'fail', injection: 'timed_out' } : { kind: 'fail' }
}
