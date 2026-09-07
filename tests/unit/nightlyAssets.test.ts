import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { verifyNightlyAssets } from '../../scripts/verify-nightly-assets.mjs'

function withAssets(test: (manifest: string, assets: string[]) => void) {
  const dir = mkdtempSync(join(tmpdir(), 'monet-nightly-assets-'))
  try {
    const mac = join(dir, 'Monet_1.2.4_aarch64.app.tar.gz')
    const windows = join(dir, 'Monet_1.2.4_x64-setup.exe')
    const dmg = join(dir, 'Monet_1.2.4_aarch64.dmg')
    const manifest = join(dir, 'nightly.json')
    const assets = [mac, `${mac}.sig`, windows, `${windows}.sig`, dmg, manifest]
    for (const file of assets.slice(0, -1)) writeFileSync(file, `fixture-${file.endsWith('.exe.sig') ? 'win' : 'mac'}`)
    execFileSync(process.execPath, [
      new URL('../../scripts/create-latest-json.mjs', import.meta.url).pathname,
      '1.2.4', mac, manifest, windows,
    ], { env: { ...process.env, RELEASE_TAG: 'nightly', RELEASE_NOTES_FILE: '' } })
    test(manifest, assets)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

describe('dual-platform Nightly assets', () => {
  it('generates installable macOS and Windows entries on the same Nightly tag', () => {
    withAssets((manifest, assets) => {
      expect(() => verifyNightlyAssets('1.2.4', manifest, assets)).not.toThrow()
      const data = JSON.parse(readFileSync(manifest, 'utf8'))
      expect(data.platforms['windows-x86_64'].url).toBe('https://github.com/zenolab124/monet/releases/download/nightly/Monet_1.2.4_x64-setup.exe')
    })
  })

  it('rejects incomplete uploads before replacing the rolling release', () => {
    withAssets((manifest, assets) => {
      expect(() => verifyNightlyAssets('1.2.4', manifest, assets.filter(file => !file.endsWith('.exe.sig')))).toThrow()
      const data = JSON.parse(readFileSync(manifest, 'utf8'))
      delete data.platforms['windows-x86_64']
      writeFileSync(manifest, JSON.stringify(data))
      expect(() => verifyNightlyAssets('1.2.4', manifest, assets)).toThrow(/macOS and Windows/)
    })
  })

  it('rejects wrong versions, stale signatures and stable-channel URLs', () => {
    withAssets((manifest, assets) => {
      expect(() => verifyNightlyAssets('1.2.5', manifest, assets)).toThrow(/version/)
      const data = JSON.parse(readFileSync(manifest, 'utf8'))
      data.platforms['windows-x86_64'].signature = 'stale-signature'
      writeFileSync(manifest, JSON.stringify(data))
      expect(() => verifyNightlyAssets('1.2.4', manifest, assets)).toThrow(/signature/)
      data.platforms['windows-x86_64'].url = data.platforms['windows-x86_64'].url.replace('/nightly/', '/v1.2.4/')
      writeFileSync(manifest, JSON.stringify(data))
      expect(() => verifyNightlyAssets('1.2.4', manifest, assets)).toThrow(/download URL/)
    })
  })
})
