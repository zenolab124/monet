import { ref, watch } from 'vue'

/**
 * 消息组虚拟化启用阈值:渲染组数(不含末组)> 阈值才启用虚拟化。
 * - 0 (默认) = 始终启用,即使只有 1 个历史组也走 tanstack-vue-virtual 路径
 * - 大值 = 小会话不启用(直接全铺),历史组数超阈值才切虚拟化
 *
 * localStorage 持久化,SessionDetail 与 SettingsView 共享同一 ref。
 */

const STORAGE_KEY = 'monet:virtualization-threshold'

function load(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw == null) return 0
    const n = Number.parseInt(raw, 10)
    return Number.isFinite(n) && n >= 0 ? n : 0
  } catch (_) {
    return 0
  }
}

const threshold = ref(load())

watch(threshold, v => {
  try {
    localStorage.setItem(STORAGE_KEY, String(v))
  } catch (_) {
    // 存储失败静默忽略,仅丢失跨启动记忆
  }
})

export function useVirtualizationSettings() {
  return { threshold }
}
