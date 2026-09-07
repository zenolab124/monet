#!/usr/bin/env node
import { readFileSync, statSync } from 'node:fs'
import { basename, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

// 发布前验证双平台清单与将要上传的文件一致，缺任一平台时保留旧 Nightly。
export function verifyNightlyAssets(version, manifestPath, assets) {
  if (assets.length !== 6 || new Set(assets.map(asset => basename(asset))).size !== 6) {
    throw new Error('Nightly requires six uniquely named assets')
  }
  for (const asset of assets) {
    if (!statSync(asset).isFile() || statSync(asset).size === 0) {
      throw new Error(`Empty or invalid Nightly asset: ${basename(asset)}`)
    }
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
  if (manifest.version !== version) throw new Error('Nightly manifest version mismatch')
  const platforms = Object.keys(manifest.platforms ?? {}).sort()
  if (platforms.join(',') !== 'darwin-aarch64,windows-x86_64') {
    throw new Error('Nightly must include macOS and Windows platforms')
  }
  const pick = (suffix) => {
    const matches = assets.filter(asset => asset.endsWith(suffix))
    if (matches.length !== 1) throw new Error(`Expected one Nightly ${suffix} asset`)
    return matches[0]
  }
  const dmg = pick('.dmg')
  const mac = pick('.app.tar.gz')
  const windows = pick('-setup.exe')
  if (resolve(pick('.json')) !== resolve(manifestPath)) throw new Error('Wrong Nightly manifest')
  for (const asset of [dmg, mac, windows]) {
    if (!basename(asset).includes(`_${version}_`)) throw new Error('Nightly artifact version mismatch')
  }
  for (const [target, asset] of [['darwin-aarch64', mac], ['windows-x86_64', windows]]) {
    const signaturePath = `${asset}.sig`
    if (!assets.includes(signaturePath)) throw new Error(`Missing signature for ${target}`)
    const entry = manifest.platforms[target]
    const url = `https://github.com/zenolab124/monet/releases/download/nightly/${basename(asset)}`
    if (entry.url !== url) throw new Error(`Wrong Nightly download URL for ${target}`)
    if (entry.signature !== readFileSync(signaturePath, 'utf8').trim()) {
      throw new Error(`Nightly signature mismatch for ${target}`)
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [version, manifest, ...assets] = process.argv.slice(2)
  verifyNightlyAssets(version, manifest, assets)
  console.log('Nightly assets and both updater platforms verified')
}
