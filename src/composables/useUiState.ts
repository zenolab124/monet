import { ref, watch } from 'vue'

/** UI 全局状态：侧栏显隐、当前区域等 */

const STORAGE_KEY = 'cc-space-ui'

/** ActivityBar 区域（v2.0.0 仅会话/首页两个可用，其余域 v2.1.0+ 点亮） */
export type AppSection = 'sessions' | 'home'

interface UiState {
  sidebarsCollapsed: boolean
  activeSection: AppSection
}

function loadState(): UiState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      return {
        sidebarsCollapsed: !!parsed.sidebarsCollapsed,
        activeSection: parsed.activeSection === 'home' ? 'home' : 'sessions',
      }
    }
  } catch (_) {}
  return { sidebarsCollapsed: false, activeSection: 'sessions' }
}

const initial = loadState()

// 项目侧栏 + 会话列表整体收起
const sidebarsCollapsed = ref<boolean>(initial.sidebarsCollapsed)

// 当前区域（重启恢复）
const activeSection = ref<AppSection>(initial.activeSection)

// 持久化
watch([sidebarsCollapsed, activeSection], ([collapsed, section]) => {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ sidebarsCollapsed: collapsed, activeSection: section }),
    )
  } catch (_) {}
})

function toggleSidebars() {
  sidebarsCollapsed.value = !sidebarsCollapsed.value
}

function switchSection(section: AppSection) {
  activeSection.value = section
}

export function useUiState() {
  return {
    sidebarsCollapsed,
    toggleSidebars,
    activeSection,
    switchSection,
  }
}
