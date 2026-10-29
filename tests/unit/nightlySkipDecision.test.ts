import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  decideNightlySkip,
  main,
  parseNightlyRelease,
  readNightlyTag,
} from '../../scripts/nightly-skip-decision.mjs'

const HEAD = 'a1b2c3d4e5f6a7b8c9d0a1b2c3d4e5f6a7b8c9d0'
const OLDER = 'ffffffe5f6a7b8c9d0a1b2c3d4e5f6a7b8c9d0aa'
const RELEASE = { version: '2.2.24', publishedAt: '2026-10-28T18:40:00Z' }

describe('decideNightlySkip', () => {
  it('skips only when HEAD is already the published Nightly commit', () => {
    expect(decideNightlySkip({
      eventName: 'schedule', headSha: HEAD, tag: { state: 'present', sha: HEAD }, release: RELEASE,
    })).toEqual({
      skip: true,
      summary: 'Skipped: HEAD a1b2c3d is already published as Nightly 2.2.24 (2026-10-28T18:40:00Z)',
    })
  })

  it('rebuilds when the tag is still on an older commit, even without new commits since', () => {
    // 上次构建失败时 tag 不会移动：无新提交也必须重试
    const decision = decideNightlySkip({
      eventName: 'schedule', headSha: HEAD, tag: { state: 'present', sha: OLDER },
    })
    expect(decision.skip).toBe(false)
    expect(decision.summary).toBe('HEAD a1b2c3d differs from the published Nightly at ffffffe; building.')
  })

  it('builds when no Nightly tag exists', () => {
    expect(decideNightlySkip({ eventName: 'schedule', headSha: HEAD, tag: { state: 'missing' } })).toEqual({
      skip: false,
      summary: 'No published Nightly found; building.',
    })
  })

  it('builds and records the reason when the tag cannot be read', () => {
    const decision = decideNightlySkip({
      eventName: 'schedule', headSha: HEAD, tag: { state: 'error', error: 'fatal: bad object' },
    })
    expect(decision.skip).toBe(false)
    expect(decision.summary).toBe('Could not read the nightly tag (fatal: bad object); building.')
  })

  it('always builds on manual dispatch', () => {
    const decision = decideNightlySkip({
      eventName: 'workflow_dispatch', headSha: HEAD, tag: { state: 'present', sha: HEAD }, release: RELEASE,
    })
    expect(decision.skip).toBe(false)
    expect(decision.summary).toBe('Triggered by workflow_dispatch; building.')
  })

  it('still skips when release metadata is unavailable', () => {
    const decision = decideNightlySkip({
      eventName: 'schedule', headSha: HEAD, tag: { state: 'present', sha: HEAD }, release: null,
    })
    expect(decision.skip).toBe(true)
    expect(decision.summary).toBe('Skipped: HEAD a1b2c3d is already published as Nightly unknown version (unknown time)')
  })
})

describe('readNightlyTag', () => {
  it('distinguishes present, missing and unreadable tags', () => {
    expect(readNightlyTag((args: string[]) => (args[0] === 'tag' ? 'nightly\n' : `${HEAD}\n`)))
      .toEqual({ state: 'present', sha: HEAD })
    expect(readNightlyTag(() => '')).toEqual({ state: 'missing' })
    expect(readNightlyTag(() => {
      throw Object.assign(new Error('Command failed'), { stderr: 'fatal: not a git repository\nmore' })
    })).toEqual({ state: 'error', error: 'fatal: not a git repository' })
  })
})

describe('parseNightlyRelease', () => {
  it('extracts the version from the release title', () => {
    expect(parseNightlyRelease({ name: 'Nightly 2.2.24', published_at: '2026-10-28T18:40:00Z' })).toEqual(RELEASE)
  })
})

describe('nightly skip entry point', () => {
  function run(env: Record<string, string>, runGh: () => string) {
    const dir = mkdtempSync(join(tmpdir(), 'monet-nightly-skip-'))
    try {
      const output = join(dir, 'output')
      const summary = join(dir, 'summary')
      writeFileSync(output, '')
      writeFileSync(summary, '')
      const runGit = (args: string[]) => {
        if (args[0] === 'tag') return 'nightly\n'
        return `${HEAD}\n`
      }
      main({ ...env, GITHUB_OUTPUT: output, GITHUB_STEP_SUMMARY: summary, GITHUB_REPOSITORY: 'owner/repo' }, { runGit, runGh })
      return { output: readFileSync(output, 'utf8'), summary: readFileSync(summary, 'utf8') }
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  }

  it('writes the skip output and summary for a scheduled run on the published commit', () => {
    const result = run({ EVENT_NAME: 'schedule' }, () => JSON.stringify({ name: 'Nightly 2.2.24', published_at: RELEASE.publishedAt }))
    expect(result.output).toBe('skip=true\n')
    expect(result.summary).toBe('Skipped: HEAD a1b2c3d is already published as Nightly 2.2.24 (2026-10-28T18:40:00Z)\n')
  })

  it('does not consult GitHub for manual runs', () => {
    const result = run({ EVENT_NAME: 'workflow_dispatch' }, () => {
      throw new Error('should not be called')
    })
    expect(result.output).toBe('skip=false\n')
  })
})
