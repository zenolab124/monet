import { defineConfig, transformerDirectives, transformerVariantGroup } from 'unocss'
import presetWind4 from '@unocss/preset-wind4'
import presetIcons from '@unocss/preset-icons'
import { icons as carbonIcons } from '@iconify-json/carbon'
import { shortcuts, rules, theme } from '../../cc-lab/style-lab/uno.config.js'

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
  shortcuts,
  rules,
  theme,
})
