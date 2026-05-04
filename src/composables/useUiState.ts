import { ref, watch } from 'vue'

/** UI 全局状态：侧栏显隐等 */

const STORAGE_KEY = 'cc-space-ui'

interface UiState {
  sidebarsCollapsed: boolean
}

function loadState(): UiState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      return {
        sidebarsCollapsed: !!parsed.sidebarsCollapsed,
      }
    }
  } catch (_) {}
  return { sidebarsCollapsed: false }
}

const initial = loadState()

// 项目侧栏 + 会话列表整体收起
const sidebarsCollapsed = ref<boolean>(initial.sidebarsCollapsed)

// 持久化
watch(sidebarsCollapsed, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ sidebarsCollapsed: v }))
  } catch (_) {}
})

function toggleSidebars() {
  sidebarsCollapsed.value = !sidebarsCollapsed.value
}

export function useUiState() {
  return {
    sidebarsCollapsed,
    toggleSidebars,
  }
}
