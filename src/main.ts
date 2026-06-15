import { createApp } from 'vue'
import 'virtual:uno.css'
import './styles/paper/paper.css'
import './styles/paper/extends.css'
import './prose.css'
import i18n from './locales'
import App from './App.vue'

createApp(App).use(i18n).mount('#app')
