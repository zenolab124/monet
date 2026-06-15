import { createI18n } from 'vue-i18n'
import zhCN from './zh-CN.json'
import enUS from './en-US.json'
import jaJP from './ja-JP.json'
import koKR from './ko-KR.json'
import frFR from './fr-FR.json'
import deDE from './de-DE.json'
import esES from './es-ES.json'
import ptBR from './pt-BR.json'
import ruRU from './ru-RU.json'
import arSA from './ar-SA.json'
import thTH from './th-TH.json'
import viVN from './vi-VN.json'

export type MessageSchema = typeof zhCN

export const builtinLocales: Record<string, { label: string; nativeLabel: string }> = {
  'zh-CN': { label: 'Chinese (Simplified)', nativeLabel: '简体中文' },
  'en-US': { label: 'English', nativeLabel: 'English' },
  'ja-JP': { label: 'Japanese', nativeLabel: '日本語' },
  'ko-KR': { label: 'Korean', nativeLabel: '한국어' },
  'fr-FR': { label: 'French', nativeLabel: 'Français' },
  'de-DE': { label: 'German', nativeLabel: 'Deutsch' },
  'es-ES': { label: 'Spanish', nativeLabel: 'Español' },
  'pt-BR': { label: 'Portuguese', nativeLabel: 'Português' },
  'ru-RU': { label: 'Russian', nativeLabel: 'Русский' },
  'ar-SA': { label: 'Arabic', nativeLabel: 'العربية' },
  'th-TH': { label: 'Thai', nativeLabel: 'ไทย' },
  'vi-VN': { label: 'Vietnamese', nativeLabel: 'Tiếng Việt' },
}

const i18n = createI18n<[MessageSchema], 'zh-CN' | 'en-US'>({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS,
  },
})

const g = i18n.global

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- 其他语言 key 可能滞后 zh-CN，靠 fallback 保底
const register = (code: string, msgs: any) => g.setLocaleMessage(code, msgs)
register('ja-JP', jaJP)
register('ko-KR', koKR)
register('fr-FR', frFR)
register('de-DE', deDE)
register('es-ES', esES)
register('pt-BR', ptBR)
register('ru-RU', ruRU)
register('ar-SA', arSA)
register('th-TH', thTH)
register('vi-VN', viVN)

export default i18n

export function registerLocale(code: string, messages: MessageSchema) {
  g.setLocaleMessage(code, messages)
}

export function setLocale(code: string) {
  ;(g.locale as unknown as { value: string }).value = code
}

export function getLocale(): string {
  return (g.locale as unknown as { value: string }).value
}
