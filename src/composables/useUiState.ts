import { ref, watch } from 'vue'
import { readMigratedStorage } from '../utils/storageMigrate'

/** UI 全局状态：侧栏显隐、当前区域等 */

const STORAGE_KEY = 'monet-ui'
const LEGACY_STORAGE_KEY = 'cc-space-ui' // 旧 key,一次性迁移读取用

/** ActivityBar 区域（v2.1.0 点亮工作台；sessions 语义为档案馆；settings 自多渠道起点亮；workshop 自 v2.3.0 点亮；automation 自 v2.4.0 点亮；search 自全局搜索起点亮） */
export type AppSection = 'workbench' | 'sessions' | 'search' | 'home' | 'settings' | 'workshop' | 'automation'

interface UiState {
  sidebarsCollapsed: boolean
  activeSection: AppSection
  projectSidebarWidth: number
  sessionListWidth: number
  monitorRailCollapsed: boolean
}

function loadState(): UiState {
  try {
    const raw = readMigratedStorage(STORAGE_KEY, LEGACY_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      const validSections: AppSection[] = ['workbench', 'sessions', 'search', 'home', 'settings', 'workshop', 'automation']
      return {
        sidebarsCollapsed: !!parsed.sidebarsCollapsed,
        activeSection: validSections.includes(parsed.activeSection)
          ? parsed.activeSection
          : 'sessions',
        projectSidebarWidth: parsed.projectSidebarWidth ?? 224,
        sessionListWidth: parsed.sessionListWidth ?? 288,
        monitorRailCollapsed: !!parsed.monitorRailCollapsed,
      }
    }
  } catch (_) {}
  return { sidebarsCollapsed: false, activeSection: 'sessions', projectSidebarWidth: 224, sessionListWidth: 288, monitorRailCollapsed: false }
}

const initial = loadState()

// 项目侧栏 + 会话列表整体收起
const sidebarsCollapsed = ref<boolean>(initial.sidebarsCollapsed)

// 当前区域（重启恢复）
const activeSection = ref<AppSection>(initial.activeSection)

// 面板宽度
const projectSidebarWidth = ref(initial.projectSidebarWidth)
const sessionListWidth = ref(initial.sessionListWidth)

// 工作台监控列收起
const monitorRailCollapsed = ref(initial.monitorRailCollapsed)

// 持久化
watch([sidebarsCollapsed, activeSection, projectSidebarWidth, sessionListWidth, monitorRailCollapsed], ([collapsed, section, pw, sw, mr]) => {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ sidebarsCollapsed: collapsed, activeSection: section, projectSidebarWidth: pw, sessionListWidth: sw, monitorRailCollapsed: mr }),
    )
  } catch (_) {}
})

function toggleSidebars() {
  sidebarsCollapsed.value = !sidebarsCollapsed.value
}

function switchSection(section: AppSection) {
  activeSection.value = section
}

const monitorPeekInstantHide = ref(false)

function toggleMonitorRail() {
  if (monitorRailCollapsed.value && monitorRailPeeking.value) {
    monitorPeekInstantHide.value = true
    monitorRailPeeking.value = false
  }
  monitorRailCollapsed.value = !monitorRailCollapsed.value
  if (!monitorRailCollapsed.value) monitorRailPeeking.value = false
  requestAnimationFrame(() => { monitorPeekInstantHide.value = false })
}

// hover 抽屉：收起态下 hover 按钮/Rail 区域时临时浮出
const monitorRailPeeking = ref(false)
let peekTimer: ReturnType<typeof setTimeout> | null = null
const PEEK_DELAY = 200

function peekMonitorRail() {
  if (!monitorRailCollapsed.value) return
  if (peekTimer) { clearTimeout(peekTimer); peekTimer = null }
  monitorRailPeeking.value = true
}

function unpeekMonitorRail() {
  if (peekTimer) clearTimeout(peekTimer)
  peekTimer = setTimeout(() => {
    monitorRailPeeking.value = false
    peekTimer = null
  }, PEEK_DELAY)
}

export function useUiState() {
  return {
    sidebarsCollapsed,
    toggleSidebars,
    activeSection,
    switchSection,
    projectSidebarWidth,
    sessionListWidth,
    monitorRailCollapsed,
    toggleMonitorRail,
    monitorRailPeeking,
    monitorPeekInstantHide,
    peekMonitorRail,
    unpeekMonitorRail,
  }
}
