import { ref, watch } from 'vue'
import { getCurrentWindow, Effect, EffectState } from '@tauri-apps/api/window'

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

  const win = getCurrentWindow()
  const tauriTheme = mode.value === 'system' ? null : mode.value === 'dark' ? 'dark' as const : 'light' as const
  win.setTheme(tauriTheme).catch(() => {})

  // 暗色：启用系统毛玻璃；亮色：关闭毛玻璃用纯色
  if (isDark) {
    win.setEffects({ effects: [Effect.UnderWindowBackground], state: EffectState.Active }).catch(() => {})
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'
  } else {
    win.clearEffects().catch(() => {})
    document.documentElement.style.background = ''
    document.body.style.background = ''
  }
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

const themeLabel: Record<ThemeMode, string> = {
  system: '跟随系统',
  light: '亮色模式',
  dark: '暗色模式',
}

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
