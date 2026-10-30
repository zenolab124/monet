import { describe, expect, it } from 'vitest'
import { createI18n } from 'vue-i18n'
import zhCN from '../../src/locales/zh-CN.json'
import enUS from '../../src/locales/en-US.json'

type Messages = { [key: string]: string | Messages }

function leafKeys(messages: Messages, prefix = ''): string[] {
  return Object.entries(messages).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return typeof value === 'string' ? [path] : leafKeys(value, path)
  })
}

// vue-i18n 在首次取用时才编译消息；`@`、`|`、`{` 用法不对会在渲染时抛错，整块组件随之失效
describe.each([
  ['zh-CN', zhCN as Messages],
  ['en-US', enUS as Messages],
])('%s 文案', (locale, messages) => {
  it('全部消息都能编译', () => {
    const i18n = createI18n({ legacy: false, locale, messages: { [locale]: messages }, missingWarn: false, fallbackWarn: false })
    const failures: string[] = []
    for (const key of leafKeys(messages)) {
      try {
        i18n.global.t(key)
      } catch (error) {
        failures.push(`${key}: ${String(error).split('\n')[0]}`)
      }
    }
    expect(failures).toEqual([])
  })
})
