import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SessionRecord } from '@/types'
import { probeSessionLoad } from '@/utils/perfProbe'

/** 每次调用创建独立实例，支持工作台多列场景 */
export function createSessionDetail() {
  const records = ref<SessionRecord[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const currentProjectId = ref<string | null>(null)
  const currentSessionId = ref<string | null>(null)

  async function loadRecords(projectId: string, sessionId: string, force = false, fallbackSessionId?: string) {
    if (!force && currentProjectId.value === projectId && currentSessionId.value === sessionId) {
      return
    }

    loading.value = true
    error.value = null
    currentProjectId.value = projectId
    currentSessionId.value = sessionId

    const probe = probeSessionLoad(sessionId)
    try {
      records.value = await invoke<SessionRecord[]>('get_session_records', {
        projectId,
        sessionId,
      })
      // 分叉垫底:自有 jsonl 未落盘(CLI 首条消息才写)时以源会话历史垫底显示,
      // 落盘后 records 非空自然走自有数据
      if (!records.value.length && fallbackSessionId) {
        try {
          records.value = await invoke<SessionRecord[]>('get_session_records', {
            projectId,
            sessionId: fallbackSessionId,
          })
        } catch (_) { /* 源会话读取失败保持空态,不算错误 */ }
      }
      probe?.afterInvoke(records.value.length)
      probe?.afterAssign()
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
