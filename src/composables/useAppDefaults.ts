import { ref, watch } from 'vue'
import type { EffortSetting } from './useSessionSettings'

const STORAGE_KEY = 'cc-space:app-defaults'

interface AppDefaults {
  effort: EffortSetting
}

function load(): AppDefaults {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { effort: null }
    const parsed = JSON.parse(raw)
    return { effort: parsed.effort ?? null }
  } catch {
    return { effort: null }
  }
}

const appDefaults = ref<AppDefaults>(load())

watch(appDefaults, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(v))
  } catch {}
}, { deep: true })

export function useAppDefaults() {
  return {
    appDefaults,
    setDefaultEffort(effort: EffortSetting) {
      appDefaults.value = { ...appDefaults.value, effort }
    },
  }
}
