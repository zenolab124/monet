import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SessionRecord } from '@/types'

/** 每次调用创建独立实例，支持分屏场景 */
export function createSessionDetail() {
  const records = ref<SessionRecord[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const currentProjectId = ref<string | null>(null)
  const currentSessionId = ref<string | null>(null)

  async function loadRecords(projectId: string, sessionId: string) {
    if (currentProjectId.value === projectId && currentSessionId.value === sessionId) {
      return
    }

    loading.value = true
    error.value = null
    currentProjectId.value = projectId
    currentSessionId.value = sessionId

    try {
      records.value = await invoke<SessionRecord[]>('get_session_records', {
        projectId,
        sessionId,
      })
    } catch (e) {
      error.value = String(e)
      records.value = []
    } finally {
      loading.value = false
    }
  }

  /** 强制重新加载（流式结束后） */
  async function reloadRecords() {
    if (currentProjectId.value && currentSessionId.value) {
      const pid = currentProjectId.value
      const sid = currentSessionId.value
      // 清除缓存让 loadRecords 重新加载
      currentProjectId.value = null
      currentSessionId.value = null
      await loadRecords(pid, sid)
    }
  }

  function clearRecords() {
    records.value = []
    currentProjectId.value = null
    currentSessionId.value = null
    error.value = null
  }

  return {
    records,
    loading,
    error,
    currentProjectId,
    currentSessionId,
    loadRecords,
    reloadRecords,
    clearRecords,
  }
}

// 向后兼容：默认单例（非分屏模式用）
const defaultInstance = createSessionDetail()
export function useSessionDetail() {
  return defaultInstance
}
