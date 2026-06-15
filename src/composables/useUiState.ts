import { ref, watch } from 'vue'

/** UI 全局状态：侧栏显隐、当前区域等 */

const STORAGE_KEY = 'cc-space-ui'

/** ActivityBar 区域（v2.1.0 点亮工作台；sessions 语义为档案馆；settings 自多渠道起点亮；workshop 自 v2.3.0 点亮；automation 自 v2.4.0 点亮） */
export type AppSection = 'workbench' | 'sessions' | 'home' | 'settings' | 'workshop' | 'automation'

interface UiState {
  sidebarsCollapsed: boolean
  activeSection: AppSection
  projectSidebarWidth: number
  sessionListWidth: number
}

function loadState(): UiState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      const validSections: AppSection[] = ['workbench', 'sessions', 'home', 'settings', 'workshop', 'automation']
      return {
        sidebarsCollapsed: !!parsed.sidebarsCollapsed,
        activeSection: validSections.includes(parsed.activeSection)
          ? parsed.activeSection
          : 'sessions',
        projectSidebarWidth: parsed.projectSidebarWidth ?? 224,
        sessionListWidth: parsed.sessionListWidth ?? 288,
      }
    }
  } catch (_) {}
  return { sidebarsCollapsed: false, activeSection: 'sessions', projectSidebarWidth: 224, sessionListWidth: 288 }
}

const initial = loadState()

// 项目侧栏 + 会话列表整体收起
const sidebarsCollapsed = ref<boolean>(initial.sidebarsCollapsed)

// 当前区域（重启恢复）
const activeSection = ref<AppSection>(initial.activeSection)

// 面板宽度
const projectSidebarWidth = ref(initial.projectSidebarWidth)
const sessionListWidth = ref(initial.sessionListWidth)

// 持久化
watch([sidebarsCollapsed, activeSection, projectSidebarWidth, sessionListWidth], ([collapsed, section, pw, sw]) => {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ sidebarsCollapsed: collapsed, activeSection: section, projectSidebarWidth: pw, sessionListWidth: sw }),
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
    projectSidebarWidth,
    sessionListWidth,
  }
}
