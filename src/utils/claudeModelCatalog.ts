// Claude Code 模型目录:以 CLI 握手自报的模型列表为准,内置 MODELS 只作兜底与容量补充。
//
// CLI 自报的列表(initialize 响应 models)含三类条目:
//   - 'default':CLI 的默认档位,不作为可选模型展示
//   - 角色别名(fable/opus/sonnet/haiku):displayName 是当前解析到的具体版本(如 "Opus 5.5")
//   - 具体版本 ID(claude-opus-5、claude-haiku-4-5-20251001 …):顺序即 CLI 推荐顺序
// 列表不含上下文容量;容量仍按内置清单推断,推断不到记 0(未知),由 result 事件真值修正。

import { MODELS, MODEL_ALIASES, findModelByCliId, inferModel, type ModelInfo } from '@/utils/modelContext'

export interface ReportedClaudeModel {
  /** 传给 CLI `--model` 的值 */
  value: string
  displayName: string
}

/** CLI 列表中的非具体版本条目:default 与角色别名(含 [1m] 写法) */
export function isClaudeRoleAlias(value: string): boolean {
  const base = value.trim().toLowerCase().replace(/\[1m\]$/, '')
  return base === 'default' || base in MODEL_ALIASES
}

/** 角色别名 → CLI 当前解析到的版本名(仅收录 displayName 有实际信息的项) */
export function roleDisplayNames(reported: readonly ReportedClaudeModel[]): Record<string, string> {
  const names: Record<string, string> = {}
  for (const model of reported) {
    const alias = model.value.trim().toLowerCase()
    const name = model.displayName.trim()
    if (alias in MODEL_ALIASES && name && name.toLowerCase() !== alias) names[alias] = name
  }
  return names
}

/**
 * 钉版本候选:CLI 报出的具体版本按其顺序列出;内置清单已知的项沿用内置条目
 * (标签带容量档位),并把同版本的 [1m] 变体排在裸 ID 之前。CLI 未报具体版本时回退内置清单。
 */
export function pinnedClaudeModels(reported: readonly ReportedClaudeModel[]): ModelInfo[] {
  const concrete = reported.filter(model => model.value.trim() && !isClaudeRoleAlias(model.value))
  if (!concrete.length) return MODELS

  const result: ModelInfo[] = []
  const seen = new Set<string>()
  const push = (model: ModelInfo) => {
    const key = model.id.toLowerCase()
    if (seen.has(key)) return
    seen.add(key)
    result.push(model)
  }
  for (const model of concrete) {
    const id = model.value.trim()
    const oneMVariant = findModelByCliId(`${id}[1m]`)
    if (oneMVariant) push(oneMVariant)
    push(findModelByCliId(id) ?? {
      id,
      label: model.displayName.trim() || id,
      contextWindow: inferModel(id)?.contextWindow ?? 0,
    })
  }
  return result
}

/** `/model` 可接受的模型值:角色别名 + 钉版本候选 + CLI 自报的全部值(小写) */
export function knownClaudeModelIds(reported: readonly ReportedClaudeModel[]): Set<string> {
  return new Set([
    ...Object.keys(MODEL_ALIASES),
    'sonnet[1m]',
    ...MODELS.map(model => model.id),
    ...pinnedClaudeModels(reported).map(model => model.id.toLowerCase()),
    ...reported.map(model => model.value.trim().toLowerCase()).filter(value => value && value !== 'default'),
  ])
}
