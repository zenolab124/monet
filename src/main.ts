import { createApp } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import 'virtual:uno.css'
import './styles/themes/_base.css'
import './styles/themes/paper.css'
import './styles/themes/ink.css'
import './styles/themes/glass.css'
import './styles/extends.css'
import './prose.css'
import i18n from './locales'
import { vTooltip } from './directives/tooltip'
import App from './App.vue'

// 全局拦截链接点击：外部 URL 用系统浏览器打开，阻止 webview 内导航
document.addEventListener('click', (e) => {
  const anchor = (e.target as HTMLElement).closest('a[href]') as HTMLAnchorElement | null
  if (!anchor) return
  const href = anchor.getAttribute('href')
  if (!href) return
  // 页内锚点、javascript: 等不拦截
  if (href.startsWith('#') || href.startsWith('javascript:')) return
  e.preventDefault()
  e.stopPropagation()
  invoke('plugin:shell|open', { path: href }).catch(() => {
    // shell 插件不可用时回退 window.open（dev 环境下）
    window.open(href, '_blank')
  })
})

createApp(App).use(i18n).directive('tooltip', vTooltip).mount('#app')
