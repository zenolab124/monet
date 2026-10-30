import { instanceKey } from './identity'
import type { EngineDescriptor, EngineHealth } from './types'

export type ChoiceEngineId = 'claude-code' | 'codex'

export interface EngineChoiceTarget {
  engineId: ChoiceEngineId
  label: string
  icon: string
  accent: 'claude' | 'codex'
}

export const ENGINE_CHOICE_TARGETS: readonly EngineChoiceTarget[] = [
  { engineId: 'claude-code', label: 'Claude Code', icon: 'i-simple-anthropic', accent: 'claude' },
  { engineId: 'codex', label: 'Codex', icon: 'i-simple-openai', accent: 'codex' },
]

export interface EngineChoiceState {
  engines: readonly EngineDescriptor[]
  health: Readonly<Record<string, EngineHealth>>
  errors: Readonly<Record<string, string>>
  loading: boolean
}

export interface EngineChoice extends EngineChoiceTarget {
  /** 选项身份:有实例时为实例 key,引擎尚未注册时为引擎 id(此时不可选) */
  id: string
  descriptor: EngineDescriptor | null
  available: boolean
  checking: boolean
}

/**
 * 新建任务与赛马换引擎共用的候选项:每个引擎实例一项,按目标引擎顺序排列。
 * 引擎尚未注册时保留一个不可选的占位项,让用户看到它在检测中或不可用。
 */
export function buildEngineChoices(
  state: EngineChoiceState,
  targets: readonly EngineChoiceTarget[] = ENGINE_CHOICE_TARGETS,
): EngineChoice[] {
  return targets.flatMap((target): EngineChoice[] => {
    const descriptors = state.engines.filter(descriptor => descriptor.instance.engineId === target.engineId)
    if (descriptors.length === 0) {
      return [{ ...target, id: target.engineId, descriptor: null, available: false, checking: state.loading }]
    }
    return descriptors.map((descriptor) => {
      const key = instanceKey(descriptor.instance)
      const runtimeHealth = state.health[key]?.runtime
      return {
        ...target,
        id: key,
        label: descriptor.displayName || target.label,
        descriptor,
        available: descriptor.enabled
          && descriptor.capabilities.runtime?.create === true
          && runtimeHealth?.available === true,
        checking: state.loading || (!runtimeHealth && !state.errors[key]),
      }
    })
  })
}
