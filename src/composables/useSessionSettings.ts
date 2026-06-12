import { ref, computed, watch, type Ref, type ComputedRef } from 'vue'

/**
 * 会话级设置(模型/努力等级/渠道)按 sessionId 持久化到 localStorage。
 * key 格式严格遵循 PRD FR-006:`cc-space:session-settings:<sid>`
 */

export type EffortLevel = 'low' | 'medium' | 'high' | 'xhigh' | 'max'

/**
 * 努力设置值:五档 effort | ultracode(超档:xhigh + 自动多智能体 workflow,
 * 经 --settings 注入而非 --effort) | null(本会话未设置,按应用默认行为)
 */
export type EffortSetting = EffortLevel | 'ultracode' | null

/**
 * 渠道切换记账(本地横线):jsonl 不存渠道(同 effort 不可还原的先例),
 * 切换事件只在 CC Space 自己的账本里,外部会话/清本地数据后无从还原
 */
export interface ChannelMark {
  /** 切换发生时历史区最后一条消息 uuid;会话起点切换为 null */
  afterUuid: string | null
  /** 切到的选择:'official' | 渠道 id | null(切回跟随默认) */
  channelId: string | null
  /** 切换时刻(ms) */
  at: number
}

export interface SessionSettings {
  /** null 表示未设置(回退使用 session 自带 model 字段) */
  modelId: string | null
  effort: EffortSetting
  /** 渠道选择:null = 跟随应用默认渠道;'official' = 强制官方;其他 = 渠道 id */
  channelId: string | null
  /** 渠道切换横线记账(按发生顺序) */
  channelMarks: ChannelMark[]
  /** 顾问模式:开启后主模型强制 sonnet + 经 --settings 注入 fable 顾问(见 settings-backlog 第 3 条) */
  advisor: boolean
}

// effort 为 null = 本会话未设置 → 按应用默认行为发送。应用默认当前实现为「不传
// --effort,由 CLI 自行决定」;将来由设置页配置(可选值含跟随 CLI/具体档位,见
// docs/settings-backlog.md),「跟随 CLI/默认值」不出现在会话级选项中
export const DEFAULT_SETTINGS: SessionSettings = {
  modelId: null,
  effort: null,
  channelId: null,
  channelMarks: [],
  advisor: false,
}

/** 顾问模式锁定的主模型(硬编码,未来设置页全局配置——见 docs/settings-backlog.md 第 3 条) */
export const ADVISOR_MAIN_MODEL = 'claude-sonnet-4-6'

const KEY_PREFIX = 'cc-space:session-settings:'

function storageKey(sid: string): string {
  return `${KEY_PREFIX}${sid}`
}

const VALID_EFFORTS: EffortLevel[] = ['low', 'medium', 'high', 'xhigh', 'max']
/** 可持久化的设置值(含 ultracode);null 不在列表内,序列化天然支持 */
const VALID_STORED: NonNullable<EffortSetting>[] = [...VALID_EFFORTS, 'ultracode']

/** 渠道 marks 宽松校验:坏条目丢弃,不作废整体(对齐 workbench drafts 增量字段先例) */
function sanitizeMarks(raw: unknown): ChannelMark[] {
  if (!Array.isArray(raw)) return []
  const marks: ChannelMark[] = []
  for (const m of raw) {
    if (!m || typeof m !== 'object') continue
    const { afterUuid, channelId, at } = m as Partial<ChannelMark>
    if (afterUuid !== null && typeof afterUuid !== 'string') continue
    if (channelId !== null && typeof channelId !== 'string') continue
    marks.push({
      afterUuid: afterUuid ?? null,
      channelId: channelId ?? null,
      at: typeof at === 'number' ? at : 0,
    })
  }
  return marks
}

function loadFromStorage(sid: string): SessionSettings {
  try {
    const raw = localStorage.getItem(storageKey(sid))
    if (!raw) return structuredClone(DEFAULT_SETTINGS)
    const parsed = JSON.parse(raw) as Partial<SessionSettings>
    const effort: EffortSetting = parsed.effort && VALID_STORED.includes(parsed.effort)
      ? parsed.effort
      : null
    const modelId = typeof parsed.modelId === 'string' && parsed.modelId.length > 0
      ? parsed.modelId
      : null
    const channelId = typeof parsed.channelId === 'string' && parsed.channelId.length > 0
      ? parsed.channelId
      : null
    return {
      modelId,
      effort,
      channelId,
      channelMarks: sanitizeMarks(parsed.channelMarks),
      advisor: parsed.advisor === true,
    }
  } catch (_) {
    return structuredClone(DEFAULT_SETTINGS)
  }
}

function saveToStorage(sid: string, settings: SessionSettings) {
  try {
    localStorage.setItem(storageKey(sid), JSON.stringify(settings))
  } catch (_) {
    // 存储失败静默忽略,设置丢失不阻塞流程
  }
}

function removeFromStorage(sid: string) {
  try {
    localStorage.removeItem(storageKey(sid))
  } catch (_) {}
}

/**
 * 只读某会话已存的渠道选择,不建 useSessionSettings 实例(供 SessionList 等
 * 列表场景的「在终端打开」带上会话渠道用)。返回 channelId 原值(null/official/id)
 */
export function readStoredChannelId(sid: string): string | null {
  return loadFromStorage(sid).channelId
}

export interface UseSessionSettingsReturn {
  /** 当前会话的设置(只读 computed) */
  settings: ComputedRef<SessionSettings>
  /** 设置模型 ID(null 表示清除) */
  setModel: (modelId: string | null) => void
  /** 设置努力等级(null 表示跟随 CLI,'ultracode' 为超档) */
  setEffort: (effort: EffortSetting) => void
  /**
   * 切换渠道并记一条横线账(afterUuid = 切换时历史区最后一条消息 uuid)。
   * 与当前值相同的切换为 no-op,不产生横线
   */
  setChannel: (channelId: string | null, afterUuid: string | null) => void
  /** 开关顾问模式 */
  setAdvisor: (advisor: boolean) => void
  /** 重置为默认并清除 localStorage */
  reset: () => void
}

/**
 * 按 sessionId 维护一份会话设置。
 * sessionId 切换时自动从 localStorage 加载,
 * 设置变化时自动写回。
 */
export function useSessionSettings(sessionId: Ref<string | null>): UseSessionSettingsReturn {
  const internal = ref<SessionSettings>(structuredClone(DEFAULT_SETTINGS))

  // sessionId 变化:重新加载
  watch(
    sessionId,
    (sid) => {
      if (sid) {
        internal.value = loadFromStorage(sid)
      } else {
        internal.value = structuredClone(DEFAULT_SETTINGS)
      }
    },
    { immediate: true },
  )

  // 设置变化:回写当前 sid
  watch(
    internal,
    (s) => {
      const sid = sessionId.value
      if (sid) saveToStorage(sid, s)
    },
    { deep: true },
  )

  function setModel(modelId: string | null) {
    internal.value = { ...internal.value, modelId }
  }

  function setEffort(effort: EffortSetting) {
    internal.value = { ...internal.value, effort }
  }

  function setChannel(channelId: string | null, afterUuid: string | null) {
    if (channelId === internal.value.channelId) return
    internal.value = {
      ...internal.value,
      channelId,
      channelMarks: [
        ...internal.value.channelMarks,
        { afterUuid, channelId, at: Date.now() },
      ],
    }
  }

  function setAdvisor(advisor: boolean) {
    internal.value = { ...internal.value, advisor }
  }

  function reset() {
    const sid = sessionId.value
    if (sid) removeFromStorage(sid)
    internal.value = structuredClone(DEFAULT_SETTINGS)
  }

  const settings = computed<SessionSettings>(() => internal.value)

  return { settings, setModel, setEffort, setChannel, setAdvisor, reset }
}
