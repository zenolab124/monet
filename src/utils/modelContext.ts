// 模型清单与上下文容量(单一事实源)
//
// 容量为 claude CLI 实测值(2026-06-11,CLI 2.1.173,result 事件 modelUsage.contextWindow):
//   - fable-5 / opus-4-8 / opus-4-7:无后缀默认即 1M([1m] 后缀为等价写法)
//   - opus-4-6 / sonnet-4-6:裸名 200K,[1m] 变体 1M
//   - haiku-4-5:200K(无 1M 支持)
//
// 1M 档的可用性因渠道而异(官方 model-config.md「Extended context」):API/按量用户全可用;
// 订阅用户 Opus 1M 在 Max/Team/Enterprise 内含,Sonnet 1M 所有订阅档均需开通 usage
// credits,未开通时发消息直接 API 报错且不降级(billing 类错误不触发 fallback)。
// 产品面向所有渠道,清单呈现**完整矩阵**、不按单一渠道裁剪;按渠道适配/隐藏不可用档
// 属设置项,见 docs/settings-backlog.md。
//
// 展示原则:主区 = 各模型最大档;200K 降级档与旧版本沉底,与主区以分割线隔开。
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
  /** 旧版本/降级档:下拉沉底,与主区之间以分割线隔开 */
  legacy?: boolean
}

/**
 * 候选模型列表(顺序即下拉展示顺序)。单一事实源:
 * ModelDropdown 选项、/model 命令校验、inferModel 匹配、容量推断都从这里派生。
 */
export const MODELS: ModelInfo[] = [
  { id: 'claude-fable-5', label: 'Fable 5', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-opus-4-8', label: 'Opus 4.8', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-sonnet-4-6[1m]', label: 'Sonnet 4.6', contextWindow: EXTENDED_CONTEXT },
  { id: 'claude-haiku-4-5', label: 'Haiku 4.5', contextWindow: DEFAULT_CONTEXT },
  { id: 'claude-opus-4-7', label: 'Opus 4.7', contextWindow: EXTENDED_CONTEXT, legacy: true },
  { id: 'claude-opus-4-6[1m]', label: 'Opus 4.6', contextWindow: EXTENDED_CONTEXT, legacy: true },
  { id: 'claude-opus-4-6', label: 'Opus 4.6 · 200K', contextWindow: DEFAULT_CONTEXT, legacy: true },
  { id: 'claude-sonnet-4-6', label: 'Sonnet 4.6 · 200K', contextWindow: DEFAULT_CONTEXT, legacy: true },
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
 *   - 无字符串(新会话未跑未选)→ 1M:当代模型默认即 1M,200K 回退几乎总是错的
 *   - `[1m]` 后缀:CLI 的 1M 模式标记,即使型号未收录也可信 → 1M
 *   - 清单匹配命中 → 取实测容量
 *   - 未收录字符串(老版本/自定义模型)→ 保守 200K(进度条宁可低估)
 */
export function getContextWindow(modelStr: string | null): number {
  if (!modelStr) return EXTENDED_CONTEXT
  if (modelStr.endsWith('[1m]')) return EXTENDED_CONTEXT
  return inferModel(modelStr)?.contextWindow ?? DEFAULT_CONTEXT
}
