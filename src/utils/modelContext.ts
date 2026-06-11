// 模型清单与上下文容量(单一事实源)
//
// 容量为 claude CLI 实测值(2026-06-11,CLI 2.1.160,result 事件 modelUsage.contextWindow):
//   - fable-5 / opus-4-8 / opus-4-7:无后缀默认即 1M([1m] 后缀为等价写法)
//   - opus-4-6 / sonnet-4-6:默认 200K,`[1m]` 后缀变体升 1M
//   - haiku-4-5:200K(无 1M 支持)
// 该矩阵随 CLI 版本/订阅档位可能漂移,漂移时以 jsonl 的 modelUsage 为准重新实测。
//
// 展示原则:裸名项 = 该模型的最大上下文档(与新模型默认一致),降级档显式标注 200K。
//
// 容量读取来源应优先用 jsonl 里 assistant.message.model 字段(真实跑过的模型字符串,
// 含 [1m] 后缀);用户在顶栏选的 id 只反映"下次发送用什么"。

export const DEFAULT_CONTEXT = 200_000
const EXTENDED_CONTEXT = 1_000_000

export interface ModelInfo {
  /** 传给 CLI `--model` 的完整模型 ID(可含 [1m] 后缀变体) */
  id: string
  /** UI 显示标签 */
  label: string
  /** 上下文窗口(CLI 实测) */
  contextWindow: number
}

/**
 * 候选模型列表(顺序即下拉展示顺序)。单一事实源:
 * ModelDropdown 选项、/model 命令校验、inferModel 匹配、容量推断都从这里派生。
 */
export const MODELS: ModelInfo[] = [
  { id: 'claude-fable-5', label: 'Fable 5', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-opus-4-8', label: 'Opus 4.8', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-opus-4-7', label: 'Opus 4.7', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-opus-4-6[1m]', label: 'Opus 4.6', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-opus-4-6', label: 'Opus 4.6 · 200K', contextWindow: DEFAULT_CONTEXT },
  { id: 'claude-sonnet-4-6[1m]', label: 'Sonnet 4.6', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-sonnet-4-6', label: 'Sonnet 4.6 · 200K', contextWindow: DEFAULT_CONTEXT },
  { id: 'claude-haiku-4-5', label: 'Haiku 4.5', contextWindow: DEFAULT_CONTEXT },
]

/**
 * 短别名 → 完整 ID。与 CLI 的别名解析行为对齐(CLI 的 sonnet 别名实测为 200K 档),
 * 用于 /model 输入与旧持久化值兼容。
 */
export const MODEL_ALIASES: Record<string, string> = {
  fable: 'claude-fable-5',
  opus: 'claude-opus-4-8',
  sonnet: 'claude-sonnet-4-6',
  haiku: 'claude-haiku-4-5',
}

/** 按 id 精确查找 */
export function findModelByCliId(cliId: string): ModelInfo | null {
  const lower = cliId.toLowerCase()
  return MODELS.find(m => m.id === lower) ?? null
}

/**
 * 把 model 字符串匹配到清单项(版本级,不做家族近似):
 * 1. 精确 id(含 [1m] 变体)
 * 2. 短别名(旧持久化值 / 用户输入)
 * 3. 版本级前缀:覆盖带日期后缀的真实字符串(claude-haiku-4-5-20251001),
 *    要求版本号边界('-' 或结尾)
 * 4. [1m] 等价写法:fable/4.8/4.7 默认即 1M,其 [1m] 后缀字符串
 *    (如 jsonl 里的 claude-fable-5[1m])归到无后缀项
 *
 * 匹配不到返回 null——调用方应显示原始字符串(自定义/未收录模型不做错误近似)。
 */
export function inferModel(modelStr: string | null): ModelInfo | null {
  if (!modelStr) return null
  const raw = modelStr.toLowerCase()

  const direct = findModelByCliId(raw)
  if (direct) return direct

  const alias = MODEL_ALIASES[raw]
  if (alias) {
    const m = findModelByCliId(alias)
    if (m) return m
  }

  const oneM = raw.endsWith('[1m]')
  const base = raw.replace(/\[1m\]$/, '')
  const versionMatch = (want1M: boolean): ModelInfo | null => {
    for (const m of MODELS) {
      if (m.id.endsWith('[1m]') !== want1M) continue
      const mBase = m.id.replace(/\[1m\]$/, '')
      if (base === mBase || base.startsWith(`${mBase}-`)) return m
    }
    return null
  }
  // 同档优先;[1m] 字符串在 [1m] 项中无果时落到无后缀项(默认 1M 模型的等价写法)
  return versionMatch(oneM) ?? (oneM ? versionMatch(false) : null)
}

/**
 * 推断给定 model 字符串的上下文容量。
 *   - `[1m]` 后缀:CLI 的 1M 模式标记,即使型号未收录也可信 → 1M
 *   - 清单匹配命中 → 取实测容量
 *   - 其余(未收录老版本/自定义模型)→ 保守 200K(进度条宁可低估)
 */
export function getContextWindow(modelStr: string | null): number {
  if (!modelStr) return DEFAULT_CONTEXT
  if (modelStr.endsWith('[1m]')) return EXTENDED_CONTEXT
  return inferModel(modelStr)?.contextWindow ?? DEFAULT_CONTEXT
}
