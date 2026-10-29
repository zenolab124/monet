import type { EngineCapabilities } from './types'

export interface WorkbenchEngineActions {
  create: boolean
  fork: boolean
  race: boolean
}

/**
 * 工作台只消费标准 capability；引擎专属按钮由各自控制器另外声明。
 * runtime 为 undefined 表示描述符未知（旧会话、未落盘草稿），原生控制器沿用本机能力；
 * 为 null 表示引擎明确没有运行时（如远端主机的只读镜像），一律不提供。
 */
export function resolveWorkbenchEngineActions(
  runtime: EngineCapabilities['runtime'] | undefined,
  nativeSurface: boolean,
): WorkbenchEngineActions {
  const inherit = nativeSurface && runtime === undefined
  const create = inherit || runtime?.create === true
  const fork = inherit || runtime?.fork === true
  return { create, fork, race: create && fork }
}
