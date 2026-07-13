import { computed, type ComputedRef } from 'vue'
import type { SessionSettings, EffortSetting } from './useSessionSettings'
import { ADVISOR_MAIN_MODEL } from './useSessionSettings'
import { useChannels, resolveChannel, OFFICIAL_CHANNEL_ID } from './useChannels'
import { useCliDefaults } from './useCliDefaults'

/**
 * 运行配置同源解析层。
 *
 * 「显示与发送同源」原则:顶栏三下拉展示的值与 sendMessage 实际发送的参数
 * 来自同一个解析结果,机制上消灭"界面显示 ≠ 实际生效"的脱节。
 *
 * 解析链(模型与思考强度同构):会话覆盖 > 渠道默认 > CLI 默认。
 *   - 渠道默认:官方渠道存渠道元数据(set_official_defaults);第三方存渠道文件
 *     本身(env.ANTHROPIC_MODEL / 顶层 effortLevel|ultracode),经 list_channels
 *     统一以 defaultModel/defaultEffort 回传
 *   - CLI 默认:落到此层时不传参数,由 CLI 自行决定;UI 展示用 useCliDefaults 读取
 */

/** 解析值来源:会话覆盖 / 渠道默认 / CLI 默认 / 顾问模式锁定 */
export type ValueSource = 'session' | 'channel' | 'cli' | 'advisor'

export interface ResolvedRunConfig {
  /** 解析后的注入渠道 id;null = 官方(零注入) */
  channelId: string | null
  /** 发送用模型;undefined = CLI 默认值也未设置 */
  model: string | undefined
  modelSource: ValueSource
  /** 发送用思考强度;undefined = CLI 默认值也未设置 */
  effort: NonNullable<EffortSetting> | undefined
  effortSource: ValueSource
  /** 当前渠道声明的默认模型/思考强度(供设置回显与下拉标注) */
  channelDefaultModel: string | null
  channelDefaultEffort: NonNullable<EffortSetting> | null
}

const VALID_EFFORT_VALUES = new Set(['low', 'medium', 'high', 'xhigh', 'max', 'ultracode'])

/** 渠道回传的 defaultEffort 是自由字符串(文件可手编),收敛到合法值域 */
function sanitizeEffort(raw: string | null | undefined): NonNullable<EffortSetting> | null {
  if (!raw) return null
  const v = raw.trim().toLowerCase()
  return VALID_EFFORT_VALUES.has(v) ? (v as NonNullable<EffortSetting>) : null
}

/**
 * 同步解析一份会话设置(纯读,无副作用)。响应式场景用 useRunConfig;
 * 即时快照场景(赛马逐 lane 广播等)直接调本函数。
 * 调用方自行保证渠道清单新鲜(发送前 refreshChannels)。
 */
export function resolveRunConfig(settings: SessionSettings): ResolvedRunConfig {
  const { channels } = useChannels()
  const { cliDefaults } = useCliDefaults()
  const channelId = resolveChannel(settings.channelId)
  // 渠道默认统一从 ChannelInfo 读:官方对应 id='official' 的条目(meta 承载)
  const info = channels.value.find(c => c.id === (channelId ?? OFFICIAL_CHANNEL_ID)) ?? null
  const channelDefaultModel = info?.defaultModel ?? null
  const channelDefaultEffort = sanitizeEffort(info?.defaultEffort)

  // 模型:顾问模式锁定 > 会话覆盖 > 渠道默认 > CLI 默认
  let model: string | undefined
  let modelSource: ValueSource
  if (settings.advisor) {
    model = ADVISOR_MAIN_MODEL
    modelSource = 'advisor'
  } else if (settings.modelId) {
    model = settings.modelId
    modelSource = 'session'
  } else if (channelDefaultModel) {
    model = channelDefaultModel
    modelSource = 'channel'
  } else {
    model = cliDefaults.value.model ?? undefined
    modelSource = 'cli'
  }

  // 思考强度:会话覆盖 > 渠道默认 > CLI 默认
  let effort: NonNullable<EffortSetting> | undefined
  let effortSource: ValueSource
  if (settings.effort) {
    effort = settings.effort
    effortSource = 'session'
  } else if (channelDefaultEffort) {
    effort = channelDefaultEffort
    effortSource = 'channel'
  } else {
    const cliEffort = cliDefaults.value.ultracode
      ? 'ultracode' as NonNullable<EffortSetting>
      : sanitizeEffort(cliDefaults.value.effort_level) ?? undefined
    effort = cliEffort
    effortSource = 'cli'
  }

  return {
    channelId,
    model,
    modelSource,
    effort,
    effortSource,
    channelDefaultModel,
    channelDefaultEffort,
  }
}

export function useRunConfig(
  settings: ComputedRef<SessionSettings>,
): { runConfig: ComputedRef<ResolvedRunConfig> } {
  const runConfig = computed<ResolvedRunConfig>(() => resolveRunConfig(settings.value))
  return { runConfig }
}
