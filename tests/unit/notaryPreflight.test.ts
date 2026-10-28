import { spawnSync } from 'node:child_process'
import { chmodSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

// 用伪造的 xcrun 驱动 build.sh --notary-preflight，不访问 Apple 接口。
const BUILD_SH = fileURLToPath(new URL('../../src-widget/build.sh', import.meta.url))
const AGREEMENT = 'Apple Developer agreement is missing or expired. Sign it at https://developer.apple.com/account as the Account Holder, then re-run this job.'
const INVALID_KEY = 'Notary API key is invalid or revoked. Check APPLE_API_PRIVATE_KEY / APPLE_API_KEY / APPLE_API_ISSUER.'
const FAKE_XCRUN = `#!/bin/bash
printf '%s\\n' "$@" > "$FAKE_LOG.args"
echo "\${APPLE_API_PRIVATE_KEY:-unset}" > "$FAKE_LOG.env"
while [ $# -gt 0 ]; do
    if [ "$1" = "--key" ]; then
        cat "$2" > "$FAKE_LOG.key"
        ls -l "$2" | cut -c1-10 > "$FAKE_LOG.mode"
    fi
    shift
done
printf '%s' "\${FAKE_OUTPUT:-}"
if [ -n "\${FAKE_SLEEP:-}" ]; then exec sleep "$FAKE_SLEEP"; fi
exit "\${FAKE_STATUS:-0}"
`

let dir: string
let log: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'monet-notary-preflight-'))
  log = join(dir, 'xcrun')
  const fake = join(dir, 'bin', 'xcrun')
  spawnSync('mkdir', ['-p', join(dir, 'bin')])
  writeFileSync(fake, FAKE_XCRUN)
  chmodSync(fake, 0o755)
})

afterEach(() => {
  rmSync(dir, { recursive: true, force: true })
})

const CREDENTIALS = {
  APPLE_API_PRIVATE_KEY: '-----BEGIN PRIVATE KEY-----\nfixture\n-----END PRIVATE KEY-----',
  APPLE_API_KEY: 'KEYID12345',
  APPLE_API_ISSUER: '00000000-0000-0000-0000-000000000000',
}

function preflight(env: Record<string, string> = {}) {
  // 从零构造环境，避免继承 CI runner 的 GITHUB_* / RUNNER_TEMP
  const result = spawnSync('bash', [BUILD_SH, '--notary-preflight'], {
    encoding: 'utf8',
    env: {
      PATH: `${join(dir, 'bin')}:/usr/bin:/bin`,
      TMPDIR: dir,
      RUNNER_TEMP: dir,
      FAKE_LOG: log,
      ...env,
    },
  })
  const lines = result.stdout.split('\n').filter(Boolean)
  return { status: result.status, stdout: result.stdout, firstLine: lines[0] ?? '', stderr: result.stderr }
}

function leftovers() {
  return readdirSync(dir).filter(name => name.startsWith('monet-notary-'))
}

describe('build.sh --notary-preflight', () => {
  it('passes and leaves no key material behind when the account is healthy', () => {
    const run = preflight({ ...CREDENTIALS, FAKE_OUTPUT: 'Successfully received submission history.' })
    expect(run.status).toBe(0)
    expect(run.firstLine).toBe('Notary preflight passed.')

    const args = readFileSync(`${log}.args`, 'utf8').split('\n')
    expect(args.slice(0, 2)).toEqual(['notarytool', 'history'])
    expect(args).toContain('KEYID12345')
    expect(args).toContain(CREDENTIALS.APPLE_API_ISSUER)
    expect(readFileSync(`${log}.key`, 'utf8')).toBe(CREDENTIALS.APPLE_API_PRIVATE_KEY)
    expect(readFileSync(`${log}.mode`, 'utf8').trim()).toBe('-rw-------')
    // 私钥不得以环境变量形式传给子进程
    expect(readFileSync(`${log}.env`, 'utf8').trim()).toBe('unset')
    expect(leftovers()).toEqual([])
  })

  it('fails with the agreement message when the response mentions the agreement', () => {
    const run = preflight({
      ...CREDENTIALS,
      FAKE_STATUS: '69',
      FAKE_OUTPUT: 'Error: A required agreement is missing or has expired.',
    })
    expect(run.status).toBe(1)
    expect(run.firstLine).toBe(AGREEMENT)
    expect(leftovers()).toEqual([])
  })

  it('treats HTTP 403 as the agreement failure', () => {
    const run = preflight({ ...CREDENTIALS, FAKE_STATUS: '1', FAKE_OUTPUT: 'Error: HTTP status code: 403. Forbidden' })
    expect(run.status).toBe(1)
    expect(run.firstLine).toBe(AGREEMENT)
  })

  it('fails with the invalid-key message on HTTP 401', () => {
    const run = preflight({ ...CREDENTIALS, FAKE_STATUS: '1', FAKE_OUTPUT: 'Error: HTTP status code: 401. Unable to authenticate.' })
    expect(run.status).toBe(1)
    expect(run.firstLine).toBe(INVALID_KEY)
    expect(leftovers()).toEqual([])
  })

  it('warns and continues on non-auth errors', () => {
    const run = preflight({
      ...CREDENTIALS,
      FAKE_STATUS: '1',
      FAKE_OUTPUT: 'Error: The network connection was lost. (code 4010)',
    })
    expect(run.status).toBe(0)
    expect(run.firstLine).toMatch(/^Notary preflight was inconclusive \(exit 1\)/)
  })

  it('warns and continues when notarytool times out', () => {
    const run = preflight({ ...CREDENTIALS, FAKE_SLEEP: '10', NOTARY_PREFLIGHT_TIMEOUT: '1' })
    expect(run.status).toBe(0)
    expect(run.firstLine).toMatch(/^Notary preflight was inconclusive/)
    expect(run.stdout).toContain('timed out after 1s')
    expect(leftovers()).toEqual([])
  })

  it('skips with a notice when credentials are missing or incomplete', () => {
    const none = preflight()
    expect(none.status).toBe(0)
    expect(none.firstLine).toBe('Notary credentials are not configured; skipping notary preflight.')
    expect(existsSync(`${log}.args`)).toBe(false)

    const partial = preflight({ APPLE_API_KEY: 'KEYID12345', APPLE_API_ISSUER: CREDENTIALS.APPLE_API_ISSUER })
    expect(partial.status).toBe(0)
    expect(partial.firstLine).toMatch(/^Notary credentials are incomplete \(2 of 3 set\)/)
    expect(existsSync(`${log}.args`)).toBe(false)
  })

  it('emits annotations and exposes the reason as a step output on GitHub Actions', () => {
    const output = join(dir, 'github-output')
    writeFileSync(output, '')
    const run = preflight({
      ...CREDENTIALS,
      GITHUB_ACTIONS: 'true',
      GITHUB_OUTPUT: output,
      FAKE_STATUS: '1',
      FAKE_OUTPUT: 'Error: HTTP status code: 403.',
    })
    expect(run.status).toBe(1)
    expect(run.firstLine).toBe(AGREEMENT)
    expect(run.stdout).toContain(`::error title=Notary preflight::${AGREEMENT}`)
    expect(readFileSync(output, 'utf8')).toBe(`reason=${AGREEMENT}\n`)

    const skipped = preflight({ GITHUB_ACTIONS: 'true' })
    expect(skipped.firstLine).toBe('::notice title=Notary preflight::Notary credentials are not configured; skipping notary preflight.')
  })
})
