// 模型清单与上下文容量(单一事实源)
//
// 分两层:
//   A. 角色层(OFFICIAL_ROLE_ITEMS) —— 四个 alias 角色(fable/opus/sonnet/haiku),
//      「始终最新」永不过期。官方渠道下拉主区呈现此层,CLI 经 alias 解析到当代版本,
//      发版换代无需改前端。sonnet 裸别名解析到 200K 档,故 sonnet 角色写 'sonnet[1m]'
//      显式钉 1M(见下方「传参语义」)。
//   B. 具体版本层(MODELS) —— 各模型的钉版本清单,用于:容量推断(jsonl 真值解析)、
//      /model 命令校验、官方渠道下拉的「钉版本沉底区」、inferModel 版本级匹配。
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
// 展示原则:主区 = 各模型最大档(或角色层);200K 降级档与旧版本沉底,与主区以分割线隔开。
//
// 容量读取来源应优先用 jsonl 里 assistant.message.model 字段(真实跑过的模型字符串,
// 含 [1m] 后缀);用户在顶栏选的 id 只反映"下次发送用什么"。
//
// 传参语义(CLI 2.1.187 实测):claude --model 接受 alias(fable/opus/sonnet/haiku)或
// 完整 ID,均可带 [1m] 后缀(sonnet[1m]、opus[1m] 合法)。ANTHROPIC_DEFAULT_X_MODEL 重定义
// alias X 的解析落点。裸 sonnet alias 解析到 200K,1M 需显式 sonnet[1m]。

import i18n from '@/locales'

export const DEFAULT_CONTEXT = 200_000
const EXTENDED_CONTEXT = 1_000_000

export interface ModelInfo {
  /** 传给 CLI `--model` 的完整模型 ID(可含 [1m] 后缀变体);角色层为 alias(fable/opus/sonnet/haiku,可带 [1m]) */
  id: string
  /** UI 显示标签 */
  label: string
  /** 上下文窗口(CLI 实测) */
  contextWindow: number
  /** 旧版本/降级档:下拉沉底,与主区之间以分割线隔开 */
  legacy?: boolean
}

/**
 * 官方渠道角色清单定义(顺序即下拉展示顺序)。四个 alias 角色永不过期,
 * CLI 经 alias 解析到当代版本;发版换代前端无需改。
 *
 * 注意:sonnet 裸别名解析到 200K 档,故此处写 'sonnet[1m]' 显式钉 1M;
 * fable/opus/haiku 裸别名容量即为其默认档,无需后缀。
 * 角色项**不标 legacy**(它们是主区、非降级档)。label 走 i18n(labelKey),
 * 副文案由 i18n 文案传达「始终最新」。
 */
const OFFICIAL_ROLE_DEFS: { id: string; labelKey: string; contextWindow: number }[] = [
  { id: 'fable', labelKey: 'topbar.roleFable', contextWindow: EXTENDED_CONTEXT },
  { id: 'opus', labelKey: 'topbar.roleOpus', contextWindow: EXTENDED_CONTEXT },
  { id: 'sonnet[1m]', labelKey: 'topbar.roleSonnet', contextWindow: EXTENDED_CONTEXT },
  { id: 'haiku', labelKey: 'topbar.roleHaiku', contextWindow: DEFAULT_CONTEXT },
]

/**
 * 官方渠道角色清单(label 在调用时解析,随 locale 切换刷新)。
 * 在响应式 computed 内调用(依赖 i18n locale)即可获得可切换语言的标签。
 */
export function officialRoleItems(): ModelInfo[] {
  return OFFICIAL_ROLE_DEFS.map(d => ({
    id: d.id,
    label: i18n.global.t(d.labelKey),
    contextWindow: d.contextWindow,
  }))
}

/**
 * 候选模型列表(顺序即下拉展示顺序)。具体版本层:
 * /model 命令校验、inferModel 匹配、容量推断、官方渠道钉版本沉底区都从这里派生。
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

/** 统一小写化 + 判断 1M 后缀(容忍 [1M]/[1m] 大小写) */
function hasOneMSuffix(s: string): boolean {
  return s.toLowerCase().endsWith('[1m]')
}

/** 按 id 精确查找 */
export function findModelByCliId(cliId: string): ModelInfo | null {
  const lower = cliId.toLowerCase()
  return MODELS.find(m => m.id === lower) ?? null
}

/**
 * 把 model 字符串匹配到清单项(版本级,不做家族近似):
 * 1. 精确 id(含 [1m] 变体)
 * 2. 短别名(旧持久化值 / 用户输入),含 alias+[1m] 后缀写法
 *    (如 'sonnet[1m]'/'opus[1m]':先剥 [1m] 再查 MODEL_ALIASES,后缀决定容量档位)
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

  const oneM = hasOneMSuffix(raw)
  const base = raw.replace(/\[1m\]$/, '')

  // 别名解析:'sonnet'/'opus'/'sonnet[1m]'/'opus[1m]' 等 —— 先剥 [1m] 再查 alias,
  // 后缀决定归位到 1M 变体还是无后缀项(版本级匹配阶段按 want1M 归位)。
  const aliasFull = MODEL_ALIASES[base]
  if (aliasFull) {
    // alias 命中:aliasFull 是无后缀版本级 id;带 [1m] 则优先取该版本的 1M 变体
    if (oneM) {
      const oneMVariant = MODELS.find(m => m.id === `${aliasFull}[1m]`)
      if (oneMVariant) return oneMVariant
    }
    const m = findModelByCliId(aliasFull)
    if (m) return m
  }

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
 *   - `[1m]` 后缀(容忍 [1M] 大小写):CLI 的 1M 模式标记,即使型号未收录也可信 → 1M
 *   - 清单匹配命中 → 取实测容量
 *   - 未收录字符串(老版本/自定义模型)→ 保守 200K(进度条宁可低估)
 */
export function getContextWindow(modelStr: string | null): number {
  if (!modelStr) return EXTENDED_CONTEXT
  if (hasOneMSuffix(modelStr)) return EXTENDED_CONTEXT
  return inferModel(modelStr)?.contextWindow ?? DEFAULT_CONTEXT
}

// ---- effort × 模型能力矩阵(UI 软提示用,不做发送拦截) ----
//
// CLI 真值(2.1.199 二进制实锤,函数 Hre/DHe/cV/VN):
//   - 官方模型按内置名单判定,不支持的档位**静默降级到 high**(不报错)
//   - xhigh 不支持:claude-3-* / opus-4-0/4-1/4-5/4-6 / sonnet-4-0/4-5/4-6 / haiku-4-5
//   - max   不支持:claude-3-* / opus-4-0/4-1/4-5 / sonnet-4-0/4-5 / haiku-4-5
//   - ultracode 要求 xhigh-capable 模型,否则静默不生效
//   - 渠道映射的 _SUPPORTED_CAPABILITIES 声明:CLI 仅在 bedrock/vertex 等 provider
//     下消费;firstParty(含 base_url 中转,即 Monet 的渠道形态)下被忽略,
//     第三方模型 effort 原样透传、由第三方 API 自行处理——故第三方模型不标注,
//     除非渠道声明了能力(Monet 自己消费声明作为 UI 标注,语义与 CLI 一致)
//
// 返回三态:true=支持 / false=不支持(CLI 会降级,UI 标「不支持」) / undefined=未知(不标注)。
// 名单可能随 CLI 更新过时,故 UI 只做软提示不拦截选择。

export interface EffortCapability {
  xhigh: boolean | undefined
  max: boolean | undefined
  ultracode: boolean | undefined
}

/** xhigh 明确不支持的官方模型(版本级前缀) */
const XHIGH_UNSUPPORTED = [
  'claude-opus-4-0', 'claude-opus-4-1', 'claude-opus-4-5', 'claude-opus-4-6',
  'claude-sonnet-4-0', 'claude-sonnet-4-5', 'claude-sonnet-4-6',
  'claude-haiku-4-5',
]
/** max 明确不支持的官方模型(版本级前缀;sonnet-4-6/opus-4-6 支持 max——矩阵非单调) */
const MAX_UNSUPPORTED = [
  'claude-opus-4-0', 'claude-opus-4-1', 'claude-opus-4-5',
  'claude-sonnet-4-0', 'claude-sonnet-4-5',
  'claude-haiku-4-5',
]

function officialSupports(base: string, unsupported: string[]): boolean | undefined {
  if (base.startsWith('claude-3-')) return false
  if (unsupported.some(m => base === m || base.startsWith(`${m}-`))) return false
  // claude-* 且不在排除名单:当代官方模型(fable-5/opus-4-7/4-8/sonnet-5 等)支持
  if (base.startsWith('claude-')) return true
  // 非 claude-*(第三方裸模型):CLI firstParty 下原样透传,未知
  return undefined
}

/**
 * 判定模型的 effort 高档能力。
 * modelEnv 为当前渠道的托管映射键(可选)。有映射时下拉 id 是角色 alias('sonnet'),
 * CLI 发送前经渠道 env 重定向到映射值再判能力——故槽匹配同时接受
 * "modelStr == 槽映射值" 与 "modelStr == 槽角色 alias";命中槽后:
 *   有 _SUPPORTED_CAPABILITIES 声明按声明(逗号分隔能力名,同 CLI cV 语义),
 *   无声明则以**映射值**为实际模型走内置名单(第三方裸模型 → undefined 不标注)。
 */
export function effortCapabilities(
  modelStr: string | null,
  modelEnv?: Record<string, string>,
): EffortCapability {
  if (!modelStr) return { xhigh: undefined, max: undefined, ultracode: undefined }
  const raw = modelStr.toLowerCase()
  let base = raw.replace(/\[1m\]$/, '')

  if (modelEnv) {
    // [槽模型键, 槽能力键, 槽角色 alias(custom 槽无 alias)]
    const slots: Array<[string, string, string | null]> = [
      ['ANTHROPIC_DEFAULT_FABLE_MODEL', 'ANTHROPIC_DEFAULT_FABLE_MODEL_SUPPORTED_CAPABILITIES', 'fable'],
      ['ANTHROPIC_DEFAULT_OPUS_MODEL', 'ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES', 'opus'],
      ['ANTHROPIC_DEFAULT_SONNET_MODEL', 'ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES', 'sonnet'],
      ['ANTHROPIC_DEFAULT_HAIKU_MODEL', 'ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES', 'haiku'],
      ['ANTHROPIC_CUSTOM_MODEL_OPTION', 'ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES', null],
    ]
    for (const [modelKey, capsKey, roleAlias] of slots) {
      const slotModel = modelEnv[modelKey]?.trim().toLowerCase()
      if (!slotModel) continue
      const hit = base === slotModel || raw === slotModel || (roleAlias !== null && base === roleAlias)
      if (!hit) continue
      const caps = modelEnv[capsKey]
      if (caps !== undefined) {
        const list = caps.toLowerCase().split(',').map(s => s.trim())
        const xhigh = list.includes('xhigh_effort')
        return { xhigh, max: list.includes('max_effort'), ultracode: xhigh }
      }
      // 无声明:实际模型是槽映射值,以它走内置名单
      base = slotModel.replace(/\[1m\]$/, '')
      break
    }
  }

  // alias 归一化到版本级 id 再查内置名单
  const normalized = MODEL_ALIASES[base] ?? base
  const xhigh = officialSupports(normalized, XHIGH_UNSUPPORTED)
  const max = officialSupports(normalized, MAX_UNSUPPORTED)
  return { xhigh, max, ultracode: xhigh }
}
