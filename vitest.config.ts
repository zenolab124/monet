import { defineConfig } from 'vitest/config'
import { fileURLToPath } from 'node:url'

// 独立于 vite.config.ts:测试只覆盖纯函数模块(PRD v2.5.0 FR-007),
// 不拉起 vue 插件与 tauri dev server 配置
export default defineConfig({
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  test: {
    include: ['tests/unit/**/*.test.ts'],
    environment: 'node',
  },
})
