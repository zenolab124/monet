import { ref, watch, computed } from 'vue'
import i18n from '../locales'
import { getCurrentWindow } from '@tauri-apps/api/window'

export type ThemeMode = 'system' | 'light' | 'dark'

const STORAGE_KEY = 'cc-space-theme'

function getStored(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY)
  if (v === 'light' || v === 'dark' || v === 'system') return v
  return 'system'
}

const mode = ref<ThemeMode>(getStored())

/** 系统偏好是否暗色 */
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)')

function applyTheme() {
  const isDark = mode.value === 'dark' || (mode.value === 'system' && prefersDark.matches)
  document.documentElement.classList.toggle('dark', isDark)

  // 同步原生窗口 chrome（标题栏红绿灯区）的亮暗，窗体本身为不透明纸底
  const win = getCurrentWindow()
  const tauriTheme = mode.value === 'system' ? null : mode.value === 'dark' ? 'dark' as const : 'light' as const
  win.setTheme(tauriTheme).catch(() => {})
}

// 监听 mode 变化
watch(mode, (v) => {
  localStorage.setItem(STORAGE_KEY, v)
  applyTheme()
}, { immediate: true })

// 监听系统主题变化
prefersDark.addEventListener('change', () => {
  if (mode.value === 'system') applyTheme()
})

/** 循环切换：system → light → dark → system */
function cycleTheme() {
  const order: ThemeMode[] = ['system', 'light', 'dark']
  const idx = order.indexOf(mode.value)
  mode.value = order[(idx + 1) % order.length]
}

const themeLabel = computed<Record<ThemeMode, string>>(() => ({
  system: i18n.global.t('theme.system'),
  light: i18n.global.t('theme.light'),
  dark: i18n.global.t('theme.dark'),
}))

const themeIcon: Record<ThemeMode, string> = {
  system: 'i-carbon-screen',
  light: 'i-carbon-sun',
  dark: 'i-carbon-moon',
}

export function useTheme() {
  return {
    mode,
    cycleTheme,
    themeLabel,
    themeIcon,
  }
}
