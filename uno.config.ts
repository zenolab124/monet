import { defineConfig, transformerDirectives, transformerVariantGroup } from 'unocss'
import presetWind4 from '@unocss/preset-wind4'
import presetIcons from '@unocss/preset-icons'
import { shortcuts, rules, theme } from '../../cc-lab/style-lab/uno.config.js'

export default defineConfig({
  presets: [
    presetWind4(),
    presetIcons({ scale: 1.2 }),
  ],
  dark: 'class',
  transformers: [
    transformerDirectives(),
    transformerVariantGroup(),
  ],
  shortcuts,
  rules,
  theme,
})
