// 模型上下文容量推断(按 claude CLI 内部行为模仿,见 PRD FR-006)
//
// claude CLI 自身不维护一张"模型→容量"映射表,而是按规则推断:
//   1) 字符串以 `[1m]` 后缀结尾(如 `claude-sonnet-4-5[1m]`) → 1,000,000
//   2) 模型为 `claude-opus-4-7`(Opus 4.7 默认启用 1M context) → 1,000,000
//   3) 其它一律 → 200,000(Anthropic API 默认上限)
//
// 读取来源应优先用 jsonl 里 assistant.message.model 字段(真实跑过的模型字符串,
// 含 [1m] 后缀);用户在 cc-space 顶栏选的别名(sonnet/opus/haiku)只反映"下次发送
// 用什么",不能直接拿来定容量(别名不带 [1m] 后缀)。

const DEFAULT_CONTEXT = 200_000
const EXTENDED_CONTEXT = 1_000_000

export interface ModelInfo {
  /** CLI `--model` 参数使用的别名:sonnet / opus / haiku */
  id: string
  /** UI 显示标签 */
  label: string
}

/** 候选模型列表(顺序即下拉展示顺序) */
export const MODELS: ModelInfo[] = [
  { id: 'sonnet', label: 'Sonnet 4.5' },
  { id: 'opus', label: 'Opus' },
  { id: 'haiku', label: 'Haiku' },
]

/** 按 CLI 别名(sonnet/opus/haiku)查找 */
export function findModelByCliId(cliId: string): ModelInfo | null {
  const lower = cliId.toLowerCase()
  return MODELS.find(m => m.id === lower) ?? null
}

/** 按 UI 标签查找(大小写不敏感,空白容忍) */
export function findModelByLabel(label: string): ModelInfo | null {
  const norm = label.trim().toLowerCase()
  return MODELS.find(m => m.label.toLowerCase() === norm) ?? null
}

/**
 * 把后端给的 model 字符串映射到 ModelInfo(用于 UI 显示)。
 *
 * 兼容形式:
 * - 别名:sonnet / opus / haiku(可带 [1m] 后缀)
 * - 完整模型名:claude-sonnet-4-5 / claude-3-5-sonnet-20241022 / claude-opus-4-7 等
 * - null / 空串 → 返回 null
 */
export function inferModel(modelStr: string | null): ModelInfo | null {
  if (!modelStr) return null
  const s = modelStr.toLowerCase().replace(/\[1m\]$/, '')

  const direct = findModelByCliId(s)
  if (direct) return direct

  if (s.includes('opus')) return findModelByCliId('opus')
  if (s.includes('sonnet')) return findModelByCliId('sonnet')
  if (s.includes('haiku')) return findModelByCliId('haiku')

  return null
}

/**
 * 推断给定 model 字符串对应的上下文容量。
 *
 * 复刻 claude CLI 内部逻辑:
 *   - `[1m]` 后缀 → 1,000,000
 *   - `claude-opus-4-7`(默认 1M)→ 1,000,000
 *   - 其它 → 200,000
 *
 * 传 null / 空串时回退到 200,000(避免进度条除 0)。
 */
export function getContextWindow(modelStr: string | null): number {
  if (!modelStr) return DEFAULT_CONTEXT
  if (modelStr.endsWith('[1m]')) return EXTENDED_CONTEXT
  if (modelStr.includes('claude-opus-4-7')) return EXTENDED_CONTEXT
  return DEFAULT_CONTEXT
}
