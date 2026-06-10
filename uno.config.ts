import { defineConfig, transformerDirectives, transformerVariantGroup } from 'unocss'
import presetWind4 from '@unocss/preset-wind4'
import presetIcons from '@unocss/preset-icons'
import { icons as carbonIcons } from '@iconify-json/carbon'
// 继承 style-lab（路径不可达时构建在此失败并报出缺失路径）；
// glass 体系（毛玻璃/渐变/霓虹 shortcuts、rules、snap-ub colors）已按 PRD v2.0.0 FR-002 废弃，
// 只保留中性布局 shortcuts。
import { shortcuts as labShortcuts } from '../../cc-lab/style-lab/uno.config.js'

const NEUTRAL_SHORTCUTS = ['center', 'flex-center']
const neutralShortcuts = (labShortcuts as [string, unknown][]).filter(
  ([name]) => NEUTRAL_SHORTCUTS.includes(name),
)

export default defineConfig({
  presets: [
    presetWind4(),
    presetIcons({
      scale: 1.2,
      extraProperties: {
        'display': 'inline-block',
        'vertical-align': 'middle',
      },
      collections: {
        carbon: () => carbonIcons,
      },
    }),
  ],
  dark: 'class',
  transformers: [
    transformerDirectives(),
    transformerVariantGroup(),
  ],
  safelist: ['i-carbon-sun', 'i-carbon-moon', 'i-carbon-screen', 'i-carbon-renew'],
  shortcuts: neutralShortcuts as never,
  rules: [
    // paper 三层棕偏阴影（铁律：阴影只用这两枚 token）
    ['shadow-paper', { 'box-shadow': 'var(--shadow-paper)' }],
    ['shadow-paper-lifted', { 'box-shadow': 'var(--shadow-paper-lifted)' }],
  ],
  theme: {
    // paper 语义 token（shadcn 命名）：颜色只存在于 CSS 变量层，此处零字面量
    colors: {
      background: 'var(--background)',
      foreground: 'var(--foreground)',
      card: {
        DEFAULT: 'var(--card)',
        foreground: 'var(--card-foreground)',
      },
      popover: {
        DEFAULT: 'var(--popover)',
        foreground: 'var(--popover-foreground)',
      },
      primary: {
        DEFAULT: 'var(--primary)',
        foreground: 'var(--primary-foreground)',
      },
      secondary: {
        DEFAULT: 'var(--secondary)',
        foreground: 'var(--secondary-foreground)',
      },
      muted: {
        DEFAULT: 'var(--muted)',
        foreground: 'var(--muted-foreground)',
      },
      accent: {
        DEFAULT: 'var(--accent)',
        foreground: 'var(--accent-foreground)',
      },
      destructive: {
        DEFAULT: 'var(--destructive)',
        foreground: 'var(--destructive-foreground)',
      },
      border: 'var(--border)',
      input: 'var(--input)',
      ring: 'var(--ring)',
      // 项目扩展 token（见 src/styles/paper/extends.css）
      claude: 'var(--claude)',
    },
    font: {
      sans: 'var(--font-sans)',
      mono: 'var(--font-mono)',
    },
    radius: {
      DEFAULT: 'var(--radius)',
    },
  },
})
