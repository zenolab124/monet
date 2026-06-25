import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SchemaDiagnosis, UsageStats } from '../types'

/**
 * 首页统计数据源（v2.2.0 FR-006 加载编排）：
 * 进首页首次惰性触发，两 command 并发、互不影响；会话期内存缓存，
 * 手动刷新强制重调。模块级单例，与 useUiState 同模式。
 */

const usage = ref<UsageStats | null>(null)
const usageLoading = ref(false)
const usageError = ref<string | null>(null)

const diag = ref<SchemaDiagnosis | null>(null)
const diagLoading = ref(false)
const diagError = ref<string | null>(null)
/** 诊断结果到达的本地时刻（FR-005 底注「上次扫描」） */
const diagAt = ref<Date | null>(null)

async function loadUsage() {
  const hasCached = usage.value !== null
  if (!hasCached) usageLoading.value = true
  usageError.value = null
  try {
    usage.value = await invoke<UsageStats>('get_usage_stats')
  } catch (e) {
    usageError.value = String(e)
  } finally {
    usageLoading.value = false
  }
}

async function loadDiag() {
  const hasCached = diag.value !== null
  if (!hasCached) diagLoading.value = true
  diagError.value = null
  try {
    diag.value = await invoke<SchemaDiagnosis>('get_schema_diagnosis')
    diagAt.value = new Date()
  } catch (e) {
    diagError.value = String(e)
  } finally {
    diagLoading.value = false
  }
}

/** 进首页时调用；首次显示 loading，后续静默后台刷新 */
function ensureLoaded() {
  if (usageLoading.value || diagLoading.value) return
  loadUsage()
  loadDiag()
}

/** 头部刷新：强制重调两个 command */
function refresh() {
  if (usageLoading.value || diagLoading.value) return
  loadUsage()
  loadDiag()
}

export function useHomeStats() {
  return {
    usage,
    usageLoading,
    usageError,
    retryUsage: loadUsage,
    diag,
    diagLoading,
    diagError,
    diagAt,
    retryDiag: loadDiag,
    ensureLoaded,
    refresh,
  }
}
