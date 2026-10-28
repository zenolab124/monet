import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  describeTrigger,
  failuresFromJobs,
  failuresFromNeeds,
  issueTitle,
  matchingOpenIssues,
  planAlert,
  renderAlert,
  reportCiStatus,
  runOutcome,
} from '../../scripts/ci-failure-alert.mjs'

const AGREEMENT = 'Apple Developer agreement is missing or expired. Sign it at https://developer.apple.com/account as the Account Holder, then re-run this job.'
const RUN_URL = 'https://github.com/owner/repo/actions/runs/42'

describe('ci failure alert decisions', () => {
  it('only treats failed jobs as failures and full success as recovery', () => {
    expect(runOutcome({ a: { result: 'success' }, b: { result: 'failure' }, c: { result: 'skipped' } })).toBe('failure')
    expect(runOutcome({ a: { result: 'success' }, b: { result: 'success' } })).toBe('success')
    // Nightly 判定跳过时构建 job 为 skipped：既不告警也不关闭
    expect(runOutcome({ prepare: { result: 'success' }, 'build-macos': { result: 'skipped' } })).toBe('none')
    expect(runOutcome({ a: { result: 'cancelled' } })).toBe('none')
    expect(runOutcome({})).toBe('none')
  })

  it('opens, comments, closes or does nothing', () => {
    const issue = { number: 7, title: 'CI failure: Nightly' }
    expect(planAlert('failure', [])).toEqual({ action: 'create', issues: [] })
    expect(planAlert('failure', [issue])).toEqual({ action: 'comment', issues: [issue] })
    expect(planAlert('success', [issue])).toEqual({ action: 'close', issues: [issue] })
    expect(planAlert('success', [])).toEqual({ action: 'none', issues: [] })
    expect(planAlert('none', [issue])).toEqual({ action: 'none', issues: [] })
  })

  it('keeps Release and Nightly alerts apart by exact title', () => {
    const issues = [
      { number: 9, title: 'CI failure: Nightly' },
      { number: 3, title: 'CI failure: Nightly' },
      { number: 5, title: 'CI failure: Release' },
      { number: 6, title: 'CI failure: Nightly', pull_request: { url: 'x' } },
    ]
    expect(matchingOpenIssues(issues, issueTitle('Nightly')).map(issue => issue.number)).toEqual([3, 9])
    expect(matchingOpenIssues(issues, issueTitle('Release')).map(issue => issue.number)).toEqual([5])
  })

  it('describes the trigger as the tag or the event name', () => {
    expect(describeTrigger({ eventName: 'push', refType: 'tag', refName: 'v2.2.24' })).toBe('tag v2.2.24')
    expect(describeTrigger({ eventName: 'schedule', refType: 'branch', refName: 'main' })).toBe('schedule')
  })

  it('extracts the failed job and step from the jobs API', () => {
    const jobs = [
      { name: 'build-windows', conclusion: 'success', steps: [] },
      {
        name: 'build-macos',
        conclusion: 'failure',
        steps: [{ name: 'Set up job', conclusion: 'success' }, { name: 'Notary preflight', conclusion: 'failure' }],
      },
      { name: 'report-ci-status', conclusion: 'failure', steps: [] },
    ]
    expect(failuresFromJobs(jobs, 'report-ci-status')).toEqual([{ job: 'build-macos', step: 'Notary preflight' }])
    expect(failuresFromNeeds({ 'build-macos': { result: 'failure' }, prepare: { result: 'success' } }))
      .toEqual([{ job: 'build-macos', step: null }])
  })

  it('puts the preflight reason first and includes no log content', () => {
    const body = renderAlert({
      reason: AGREEMENT,
      workflow: 'Nightly',
      runUrl: RUN_URL,
      trigger: 'schedule',
      failures: [{ job: 'build-macos', step: 'Notary preflight' }],
    })
    expect(body.split('\n\n')[0]).toBe(AGREEMENT)
    expect(body).toBe([
      AGREEMENT,
      '',
      '- Workflow: Nightly',
      `- Run: ${RUN_URL}`,
      '- Trigger: schedule',
      '- Failed:',
      '  - `build-macos` / `Notary preflight`',
    ].join('\n'))
    expect(renderAlert({ reason: '', workflow: 'Release', runUrl: RUN_URL, trigger: 'tag v1.0.0', failures: [] }))
      .toMatch(/^- Workflow: Release\n/)
  })
})

describe('reportCiStatus', () => {
  let dir: string
  let summaryFile: string

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'monet-ci-alert-'))
    summaryFile = join(dir, 'summary')
    writeFileSync(summaryFile, '')
  })

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true })
  })

  function env(needs: Record<string, unknown>, workflow = 'Nightly') {
    return {
      NEEDS_JSON: JSON.stringify(needs),
      GITHUB_WORKFLOW: workflow,
      GITHUB_REPOSITORY: 'owner/repo',
      GITHUB_SERVER_URL: 'https://github.com',
      GITHUB_RUN_ID: '42',
      GITHUB_RUN_ATTEMPT: '1',
      GITHUB_EVENT_NAME: 'schedule',
      GITHUB_REF_TYPE: 'branch',
      GITHUB_REF_NAME: 'main',
      GITHUB_STEP_SUMMARY: summaryFile,
      ALERT_JOB_NAME: 'report-ci-status',
    }
  }

  // 伪造 gh：按参数路由响应并记录调用，不访问 GitHub
  function fakeGh(routes: { openIssues?: object[], labelMissing?: boolean, jobsFail?: boolean }) {
    const calls: string[][] = []
    const gh = (args: string[]) => {
      calls.push(args)
      const path = args.find(arg => arg.startsWith('repos/')) ?? ''
      if (path.includes('/issues?state=open')) return (routes.openIssues ?? []).map(issue => JSON.stringify(issue)).join('\n')
      if (path.endsWith('/labels/ci-failure')) {
        if (routes.labelMissing) throw Object.assign(new Error('failed'), { stderr: 'gh: Not Found (HTTP 404)' })
        return '{}'
      }
      if (path.includes('/jobs')) {
        if (routes.jobsFail) throw new Error('HTTP 403')
        return JSON.stringify([{ name: 'build-macos', conclusion: 'failure', steps: [{ name: 'Notary preflight', conclusion: 'failure' }] }])
      }
      if (path === 'repos/owner/repo/issues') return 'https://github.com/owner/repo/issues/11\n'
      return '{}'
    }
    return { gh, calls }
  }

  const failedNeeds = {
    prepare: { result: 'success', outputs: {} },
    'build-macos': { result: 'failure', outputs: { notary_preflight_reason: AGREEMENT } },
  }

  it('creates the label and a new issue on first failure', () => {
    const { gh, calls } = fakeGh({ labelMissing: true })
    expect(reportCiStatus(env(failedNeeds), gh)).toBe(true)
    const labelCreate = calls.findIndex(args => args.includes('repos/owner/repo/labels'))
    const issueCreate = calls.findIndex(args => args.includes('repos/owner/repo/issues'))
    expect(labelCreate).toBeGreaterThan(-1)
    expect(issueCreate).toBeGreaterThan(labelCreate)
    const create = calls[issueCreate]
    expect(create).toContain('title=CI failure: Nightly')
    expect(create).toContain('labels[]=ci-failure')
    const body = create.find(arg => arg.startsWith('body='))!
    expect(body.startsWith(`body=${AGREEMENT}\n\n`)).toBe(true)
    expect(body).toContain('`build-macos` / `Notary preflight`')
    expect(readFileSync(summaryFile, 'utf8')).toContain('opened https://github.com/owner/repo/issues/11')
  })

  it('comments on the existing open issue instead of opening another', () => {
    const { gh, calls } = fakeGh({ openIssues: [{ number: 7, title: 'CI failure: Nightly', pull_request: null }], jobsFail: true })
    reportCiStatus(env(failedNeeds), gh)
    expect(calls.some(args => args.includes('repos/owner/repo/issues'))).toBe(false)
    const comment = calls.find(args => args.includes('repos/owner/repo/issues/7/comments'))!
    // jobs API 不可用时退回 needs：只列 job
    expect(comment.find(arg => arg.startsWith('body='))).toContain('  - `build-macos`')
  })

  it('comments Recovered and closes after a fully successful run', () => {
    const { gh, calls } = fakeGh({ openIssues: [{ number: 5, title: 'CI failure: Release', pull_request: null }] })
    reportCiStatus(env({
      'validate-release-notes': { result: 'success' },
      'build-windows': { result: 'success' },
      'build-macos': { result: 'success' },
    }, 'Release'), gh)
    const comment = calls.find(args => args.includes('repos/owner/repo/issues/5/comments'))!
    expect(comment).toContain(`body=Recovered: ${RUN_URL}`)
    const close = calls.find(args => args.includes('repos/owner/repo/issues/5') && args.includes('PATCH'))!
    expect(close).toContain('state=closed')
  })

  it('does nothing for a skipped Nightly run', () => {
    const { gh, calls } = fakeGh({ openIssues: [{ number: 7, title: 'CI failure: Nightly', pull_request: null }] })
    reportCiStatus(env({ prepare: { result: 'success' }, 'build-windows': { result: 'skipped' }, 'build-macos': { result: 'skipped' } }), gh)
    expect(calls).toEqual([])
  })

  it('records an API failure in the run summary without throwing', () => {
    const gh = () => {
      throw Object.assign(new Error('failed'), { stderr: 'HTTP 502: Bad Gateway\ndetails' })
    }
    expect(reportCiStatus(env(failedNeeds), gh)).toBe(false)
    expect(readFileSync(summaryFile, 'utf8')).toBe('CI failure alert could not be delivered: HTTP 502: Bad Gateway\n')
  })
})
