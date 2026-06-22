import { ref, computed, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import i18n from '../locales'
import { THEMES, getTheme, type ThemeMeta } from './themeRegistry'

const STORAGE_KEY = 'cc-space-theme'

interface ThemeConfig {
  version: 2
  lightTheme: string
  darkTheme: string
}

function loadConfig(): ThemeConfig {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return { version: 2, lightTheme: 'paper', darkTheme: 'ink' }

  if (raw === 'system') return { version: 2, lightTheme: 'paper', darkTheme: 'ink' }
  if (raw === 'light') return { version: 2, lightTheme: 'paper', darkTheme: 'paper' }
  if (raw === 'dark') return { version: 2, lightTheme: 'ink', darkTheme: 'ink' }

  try {
    const parsed = JSON.parse(raw)
    if (parsed.version === 2) return parsed
  } catch {}
  return { version: 2, lightTheme: 'paper', darkTheme: 'ink' }
}

const config = ref<ThemeConfig>(loadConfig())

const prefersDark = ref(window.matchMedia('(prefers-color-scheme: dark)').matches)
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
  prefersDark.value = e.matches
})

const activeTheme = computed<ThemeMeta>(() => {
  const id = prefersDark.value ? config.value.darkTheme : config.value.lightTheme
  return getTheme(id)
})

function applyTheme() {
  const theme = activeTheme.value
  const html = document.documentElement
  const body = document.body

  THEMES.forEach(t => html.classList.remove(t.className))
  html.classList.add(theme.className)

  html.classList.toggle('dark', theme.isDark)

  const atmosphereClasses = THEMES.map(t => t.atmosphere).filter(Boolean) as string[]
  atmosphereClasses.forEach(cls => body.classList.remove(cls))
  if (theme.atmosphere) body.classList.add(theme.atmosphere)

  const win = getCurrentWindow()
  win.setTheme(theme.isDark ? 'dark' : 'light').catch(() => {})
}

watch([config, prefersDark], () => {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(config.value))
  applyTheme()
}, { immediate: true, deep: true })

function setLightTheme(themeId: string) {
  config.value = { ...config.value, lightTheme: themeId }
}

function setDarkTheme(themeId: string) {
  config.value = { ...config.value, darkTheme: themeId }
}

function cycleActiveTheme() {
  const currentId = prefersDark.value ? config.value.darkTheme : config.value.lightTheme
  const idx = THEMES.findIndex(t => t.id === currentId)
  const nextId = THEMES[(idx + 1) % THEMES.length].id
  if (prefersDark.value) {
    config.value = { ...config.value, darkTheme: nextId }
  } else {
    config.value = { ...config.value, lightTheme: nextId }
  }
}

const activeThemeLabel = computed(() => i18n.global.t(activeTheme.value.labelKey))

export function useTheme() {
  return {
    config,
    activeTheme,
    activeThemeLabel,
    prefersDark,
    themes: THEMES,
    setLightTheme,
    setDarkTheme,
    cycleActiveTheme,
  }
}
