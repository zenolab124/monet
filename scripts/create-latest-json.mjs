#!/usr/bin/env node
// 生成 Tauri updater 的版本清单 latest.json。
// 用法: node create-latest-json.mjs <version> <tarball-path> <out-path>
// 上传到 GitHub Release 后,应用内 updater 经 tauri.conf plugins.updater.endpoints
// 的 /releases/latest/download/latest.json 读取(draft release 不算 latest,
// 手动 publish 后才对用户生效)。
import { readFileSync, writeFileSync } from 'node:fs'
import { basename } from 'node:path'

const [version, tarball, outPath] = process.argv.slice(2)
if (!version || !tarball || !outPath) {
  console.error('usage: create-latest-json.mjs <version> <tarball> <out>')
  process.exit(1)
}

// .sig 内容整体作为 signature 字段(tauri signer sign 的产物,base64 文本)
const signature = readFileSync(`${tarball}.sig`, 'utf8').trim()

const manifest = {
  version,
  pub_date: new Date().toISOString(),
  platforms: {
    'darwin-aarch64': {
      signature,
      url: `https://github.com/zenolab124/monet/releases/download/v${version}/${basename(tarball)}`,
    },
  },
}

writeFileSync(outPath, `${JSON.stringify(manifest, null, 2)}\n`)
console.log(`latest.json → ${outPath}`)
