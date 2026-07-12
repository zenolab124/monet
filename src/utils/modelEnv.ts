// 渠道「模型角色映射」的 env 命名空间契约(前后端严格一致)。
//
// 渠道 channels/<id>.json 顶层 env 块内,由 Monet 托管的 21 个模型映射键。
// Rust 侧 save_channel 的 modelEnv 参数按「整命名空间替换」语义写入这些键;
// ChannelView.modelEnv 从 env 块过滤出这些键当前值回传(明文,模型 ID 非敏感)。
//
// [1m] 后缀约定:CLI 大小写不敏感(内部 toLowerCase 后 includes("[1m]")),
// 我方统一生成小写 [1m];解析/回显时容忍 [1M]。不使用 SUPPORTED_CAPABILITIES 表达 1M。

/** 四个 alias 角色 */
export const MODEL_ROLES = ['FABLE', 'OPUS', 'SONNET', 'HAIKU'] as const
export type ModelRole = (typeof MODEL_ROLES)[number]

/** 角色对应的裸 alias(下拉项 id 用;CLI 经渠道 env 重定向,无需 [1m]) */
export const ROLE_ALIAS: Record<ModelRole, string> = {
  FABLE: 'fable',
  OPUS: 'opus',
  SONNET: 'sonnet',
  HAIKU: 'haiku',
}

/** 每个角色的托管键(v1 只在 UI 用 MODEL 与 _NAME,后两者保留命名空间不做 UI) */
export function roleModelKey(role: ModelRole): string {
  return `ANTHROPIC_DEFAULT_${role}_MODEL`
}
export function roleNameKey(role: ModelRole): string {
  return `ANTHROPIC_DEFAULT_${role}_MODEL_NAME`
}
export function roleDescKey(role: ModelRole): string {
  return `ANTHROPIC_DEFAULT_${role}_MODEL_DESCRIPTION`
}
export function roleCapsKey(role: ModelRole): string {
  return `ANTHROPIC_DEFAULT_${role}_MODEL_SUPPORTED_CAPABILITIES`
}

/** 自定义第五槽(子键在 _OPTION 之后追加后缀,与 CLI 二进制内实际键名一致) */
export const CUSTOM_MODEL_OPTION = 'ANTHROPIC_CUSTOM_MODEL_OPTION'
export const CUSTOM_MODEL_NAME = 'ANTHROPIC_CUSTOM_MODEL_OPTION_NAME'
export const CUSTOM_MODEL_DESCRIPTION = 'ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION'
export const CUSTOM_MODEL_SUPPORTED_CAPABILITIES = 'ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES'

/** 兜底键:请求未落到任何角色时使用的默认模型 */
export const ANTHROPIC_MODEL = 'ANTHROPIC_MODEL'

/** 全部 21 个托管键(与 Rust 侧移除/过滤集一致) */
export const MODEL_ENV_KEYS: string[] = [
  ...MODEL_ROLES.flatMap(role => [
    roleModelKey(role),
    roleNameKey(role),
    roleDescKey(role),
    roleCapsKey(role),
  ]),
  CUSTOM_MODEL_OPTION,
  CUSTOM_MODEL_NAME,
  CUSTOM_MODEL_DESCRIPTION,
  CUSTOM_MODEL_SUPPORTED_CAPABILITIES,
  ANTHROPIC_MODEL,
]

/** 判断模型值是否含 1M 后缀(大小写不敏感) */
export function valueHasOneM(value: string | undefined | null): boolean {
  return !!value && value.toLowerCase().endsWith('[1m]')
}

/** 剥掉 [1m]/[1M] 后缀,还原裸模型值 */
export function stripOneM(value: string): string {
  return value.replace(/\[1m\]$/i, '')
}

/** 给裸模型值追加小写 [1m](若勾选 1M 且尚无后缀) */
export function withOneM(value: string, oneM: boolean): string {
  const base = stripOneM(value.trim())
  if (!base) return ''
  return oneM ? `${base}[1m]` : base
}

/** 该 modelEnv 是否已配置任一角色映射或自定义槽(判断第三方渠道是否有映射) */
export function hasAnyMapping(modelEnv: Record<string, string> | undefined | null): boolean {
  if (!modelEnv) return false
  for (const role of MODEL_ROLES) {
    if (modelEnv[roleModelKey(role)]?.trim()) return true
  }
  return !!modelEnv[CUSTOM_MODEL_OPTION]?.trim()
}

/** 表单单行:实际请求模型 + 显示名称 + 1M 勾选 */
export interface ModelMapRow {
  /** 实际请求模型(裸值,不含 [1m]:1M 状态由 oneM 表达) */
  model: string
  /** 显示名称(可空,非空才写 _NAME) */
  name: string
  /** 1M 勾选 */
  oneM: boolean
}

/** 表单聚合状态:四角色 + 自定义槽 + 兜底模型 */
export interface ModelMapForm {
  roles: Record<ModelRole, ModelMapRow>
  custom: ModelMapRow
  /** ANTHROPIC_MODEL 兜底模型(原样值,可含 [1m] 后缀——本行无 1M 勾选框) */
  fallback: string
}

function emptyRow(): ModelMapRow {
  return { model: '', name: '', oneM: false }
}

/** 空白表单(新建渠道 / 无映射) */
export function emptyModelMapForm(): ModelMapForm {
  return {
    roles: {
      FABLE: emptyRow(),
      OPUS: emptyRow(),
      SONNET: emptyRow(),
      HAIKU: emptyRow(),
    },
    custom: emptyRow(),
    fallback: '',
  }
}

/** 从 env 块(modelEnv)解析回表单状态:剥 [1m]/[1M] 还原勾选 */
export function parseModelEnv(modelEnv: Record<string, string> | undefined | null): ModelMapForm {
  const form = emptyModelMapForm()
  if (!modelEnv) return form
  for (const role of MODEL_ROLES) {
    const rawModel = modelEnv[roleModelKey(role)]?.trim() ?? ''
    if (rawModel) {
      form.roles[role] = {
        model: stripOneM(rawModel),
        name: modelEnv[roleNameKey(role)]?.trim() ?? '',
        oneM: valueHasOneM(rawModel),
      }
    }
  }
  const rawCustom = modelEnv[CUSTOM_MODEL_OPTION]?.trim() ?? ''
  if (rawCustom) {
    form.custom = {
      model: stripOneM(rawCustom),
      name: modelEnv[CUSTOM_MODEL_NAME]?.trim() ?? '',
      oneM: valueHasOneM(rawCustom),
    }
  }
  // 兜底行无 1M 勾选框,不剥 [1m] 后缀(剥了无处还原,会在下次保存时静默丢失)——原样回显
  form.fallback = modelEnv[ANTHROPIC_MODEL]?.trim() ?? ''
  return form
}

/**
 * 从表单状态生成 env 键值(整命名空间替换语义:只产出非空键)。
 *   - 模型值非空才写该角色 _MODEL(拼 [1m] 后缀);显示名非空才写 _NAME
 *   - 自定义槽同理,写 CUSTOM_MODEL_OPTION / _NAME
 *   - fallback 非空写 ANTHROPIC_MODEL
 * 全部行清空 → 返回 {} (清除全部映射,配合 Rust 整命名空间替换语义即删除这些键)。
 */
export function buildModelEnv(form: ModelMapForm): Record<string, string> {
  const env: Record<string, string> = {}
  for (const role of MODEL_ROLES) {
    const row = form.roles[role]
    const model = row.model.trim()
    if (!model) continue
    env[roleModelKey(role)] = withOneM(model, row.oneM)
    const name = row.name.trim()
    if (name) env[roleNameKey(role)] = name
  }
  const customModel = form.custom.model.trim()
  if (customModel) {
    env[CUSTOM_MODEL_OPTION] = withOneM(customModel, form.custom.oneM)
    const customName = form.custom.name.trim()
    if (customName) env[CUSTOM_MODEL_NAME] = customName
  }
  const fallback = form.fallback.trim()
  if (fallback) env[ANTHROPIC_MODEL] = fallback
  return env
}
