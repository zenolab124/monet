import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { HooksOverview } from '../types'

/**
 * Hooks 数据源（v2.9.0 FR-005）：
 * 模块级单例（与 useWorkshop 同模式）——WorkshopView 对子页用 v-if 挂载，
 * 状态放组件内会随切换卸载丢失；提升到 module 级后「一次缓存 + 手动刷新」成立，
 * 子导航计数（hooksCount）也不依赖组件实例存活。
 */

const overview = ref<HooksOverview | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

let loadedOnce = false

async function loadHooks() {
  if (loading.value) return
  loading.value = true
  error.value = null
  try {
    overview.value = await invoke<HooksOverview>('get_hooks_overview')
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

/** 首次进入时调用；已加载则用缓存 */
function ensureLoaded() {
  if (loadedOnce) return
  loadedOnce = true
  loadHooks()
}

/** hooks 总条数（子导航计数）；未加载为 null（前端显示「…」） */
const hooksCount = computed<number | null>(() => {
  if (!overview.value) return null
  return overview.value.events.reduce((sum, g) => sum + g.entries.length, 0)
})

export function useHooks() {
  return { overview, loading, error, loadHooks, ensureLoaded, hooksCount }
}
