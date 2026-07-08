export interface ThemeMeta {
  id: string
  className: string
  isDark: boolean
  atmosphere: string | false
  icon: string
  labelKey: string
}

export const THEMES: ThemeMeta[] = [
  {
    id: 'paper',
    className: 'theme-paper',
    isDark: false,
    atmosphere: 'paper-atmosphere',
    icon: 'i-carbon-sun',
    labelKey: 'theme.paper',
  },
  {
    id: 'ink',
    className: 'theme-ink',
    isDark: true,
    atmosphere: false,
    icon: 'i-carbon-moon',
    labelKey: 'theme.ink',
  },
]

export function getTheme(id: string): ThemeMeta {
  return THEMES.find(t => t.id === id) ?? THEMES[0]
}
