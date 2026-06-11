import { ref, computed, watch, type Ref, type ComputedRef } from 'vue'

/**
 * 会话级设置(模型/努力等级)按 sessionId 持久化到 localStorage。
 * key 格式严格遵循 PRD FR-006:`cc-space:session-settings:<sid>`
 */

export type EffortLevel = 'low' | 'medium' | 'high' | 'xhigh' | 'max'

/**
 * 努力设置值:五档 effort | ultracode(超档:xhigh + 自动多智能体 workflow,
 * 经 --settings 注入而非 --effort) | null(本会话未设置,按应用默认行为)
 */
export type EffortSetting = EffortLevel | 'ultracode' | null

export interface SessionSettings {
  /** null 表示未设置(回退使用 session 自带 model 字段) */
  modelId: string | null
  effort: EffortSetting
}

// effort 为 null = 本会话未设置 → 按应用默认行为发送。应用默认当前实现为「不传
// --effort,由 CLI 自行决定」;将来由设置页配置(可选值含跟随 CLI/具体档位,见
// docs/settings-backlog.md),「跟随 CLI/默认值」不出现在会话级选项中
export const DEFAULT_SETTINGS: SessionSettings = {
  modelId: null,
  effort: null,
}

const KEY_PREFIX = 'cc-space:session-settings:'

function storageKey(sid: string): string {
  return `${KEY_PREFIX}${sid}`
}

const VALID_EFFORTS: EffortLevel[] = ['low', 'medium', 'high', 'xhigh', 'max']
/** 可持久化的设置值(含 ultracode);null 不在列表内,序列化天然支持 */
const VALID_STORED: NonNullable<EffortSetting>[] = [...VALID_EFFORTS, 'ultracode']

function loadFromStorage(sid: string): SessionSettings {
  try {
    const raw = localStorage.getItem(storageKey(sid))
    if (!raw) return { ...DEFAULT_SETTINGS }
    const parsed = JSON.parse(raw) as Partial<SessionSettings>
    const effort: EffortSetting = parsed.effort && VALID_STORED.includes(parsed.effort)
      ? parsed.effort
      : null
    const modelId = typeof parsed.modelId === 'string' && parsed.modelId.length > 0
      ? parsed.modelId
      : null
    return { modelId, effort }
  } catch (_) {
    return { ...DEFAULT_SETTINGS }
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

export interface UseSessionSettingsReturn {
  /** 当前会话的设置(只读 computed) */
  settings: ComputedRef<SessionSettings>
  /** 设置模型 ID(null 表示清除) */
  setModel: (modelId: string | null) => void
  /** 设置努力等级(null 表示跟随 CLI,'ultracode' 为超档) */
  setEffort: (effort: EffortSetting) => void
  /** 重置为默认并清除 localStorage */
  reset: () => void
}

/**
 * 按 sessionId 维护一份会话设置。
 * sessionId 切换时自动从 localStorage 加载,
 * 设置变化时自动写回。
 */
export function useSessionSettings(sessionId: Ref<string | null>): UseSessionSettingsReturn {
  const internal = ref<SessionSettings>({ ...DEFAULT_SETTINGS })

  // sessionId 变化:重新加载
  watch(
    sessionId,
    (sid) => {
      if (sid) {
        internal.value = loadFromStorage(sid)
      } else {
        internal.value = { ...DEFAULT_SETTINGS }
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

  function reset() {
    const sid = sessionId.value
    if (sid) removeFromStorage(sid)
    internal.value = { ...DEFAULT_SETTINGS }
  }

  const settings = computed<SessionSettings>(() => internal.value)

  return { settings, setModel, setEffort, reset }
}
