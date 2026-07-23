#!/usr/bin/env node
// 从 models.dev 拉取模型价目，按 provider 白名单精简后写入内置快照
// （src-tauri/src/pricing-snapshot.json，编译期 include_str! 进二进制作离线兜底）。
// 发版前可手动执行刷新快照：node scripts/update-pricing-snapshot.mjs

import { writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const API_URL = 'https://models.dev/api.json'
const OUT = join(dirname(fileURLToPath(import.meta.url)), '../src-tauri/src/pricing-snapshot.json')

// 白名单：仅官方 provider（含 Claude Code 生态常见第三方的官方价），
// 顺序即优先级——同名模型先到先得，聚合网关的镜像价一律不收。
// 与 src-tauri/src/pricing.rs 的 PROVIDER_ALLOWLIST 保持一致。
const PROVIDERS = [
  'anthropic', 'openai', 'google',
  'zhipuai', 'zai', 'moonshotai', 'moonshotai-cn',
  'deepseek', 'minimax', 'minimax-cn',
  'alibaba', 'alibaba-cn', 'xai',
]

const res = await fetch(API_URL, { signal: AbortSignal.timeout(30_000) })
if (!res.ok) throw new Error(`fetch ${API_URL} -> ${res.status}`)
const api = await res.json()

const models = {}
let kept = 0
for (const provider of PROVIDERS) {
  const entry = api[provider]
  if (!entry?.models) continue
  for (const [id, m] of Object.entries(entry.models)) {
    const c = m?.cost
    if (!c || typeof c.input !== 'number' || typeof c.output !== 'number') continue
    const key = id.toLowerCase()
    if (models[key]) continue
    models[key] = {
      input: c.input,
      output: c.output,
      // 数据源缺缓存单价时按 Anthropic 比例兜底：写 1.25x、读 0.1x
      cache_write: typeof c.cache_write === 'number' ? c.cache_write : c.input * 1.25,
      cache_read: typeof c.cache_read === 'number' ? c.cache_read : c.input * 0.1,
    }
    kept++
  }
}

const snapshot = {
  fetched_at: Math.floor(Date.now() / 1000),
  source: API_URL,
  models,
}
writeFileSync(OUT, JSON.stringify(snapshot, null, 1) + '\n')
console.log(`pricing snapshot: ${kept} models -> ${OUT}`)
