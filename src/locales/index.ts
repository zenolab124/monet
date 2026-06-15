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

export type LocaleCode = string
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

const i18n = createI18n<[MessageSchema], string>({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS,
    'ja-JP': jaJP,
    'ko-KR': koKR,
    'fr-FR': frFR,
    'de-DE': deDE,
    'es-ES': esES,
    'pt-BR': ptBR,
    'ru-RU': ruRU,
    'ar-SA': arSA,
    'th-TH': thTH,
    'vi-VN': viVN,
  },
})

export default i18n

export function registerLocale(code: string, messages: MessageSchema) {
  i18n.global.setLocaleMessage(code, messages)
}

export function setLocale(code: string) {
  ;(i18n.global.locale as unknown as { value: string }).value = code
}

export function getLocale(): string {
  return (i18n.global.locale as unknown as { value: string }).value
}
