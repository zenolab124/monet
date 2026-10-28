#!/usr/bin/env node
// 发布链路失败告警：失败时开 / 追加 ci-failure Issue，完整成功后评论恢复并关闭。
// Issue 只写 workflow 名、运行链接、触发来源、失败的 job / 步骤与预检原因句，
// 不粘贴任何日志片段。告警自身出错只记入运行摘要，绝不改变原 workflow 结论。
import { execFileSync } from 'node:child_process'
import { appendFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

export const ALERT_LABEL = 'ci-failure'
const LABEL_COLOR = 'B60205'
const LABEL_DESCRIPTION = 'Release or Nightly workflow failure, opened automatically'

export function issueTitle(workflow) {
  return `CI failure: ${workflow}`
}

/** 只有 failure 算失败；全部 success 才算完整成功；跳过 / 取消不触发任何动作。 */
export function runOutcome(needs) {
  const results = Object.values(needs ?? {}).map(job => job?.result)
  if (!results.length) return 'none'
  if (results.includes('failure')) return 'failure'
  if (results.every(result => result === 'success')) return 'success'
  return 'none'
}

export function describeTrigger({ eventName, refType, refName }) {
  if (eventName === 'push' && refType === 'tag') return `tag ${refName}`
  return eventName || 'unknown'
}

/** 从 Actions jobs API 的返回中取失败的 job 与其第一个失败步骤。 */
export function failuresFromJobs(jobs, excludeJob) {
  return (jobs ?? [])
    .filter(job => job.conclusion === 'failure' && job.name !== excludeJob)
    .map(job => ({
      job: job.name,
      step: (job.steps ?? []).find(step => step.conclusion === 'failure')?.name ?? null,
    }))
}

/** jobs API 不可用时退回 needs 上下文：只知道 job，不知道步骤。 */
export function failuresFromNeeds(needs) {
  return Object.entries(needs ?? {})
    .filter(([, job]) => job?.result === 'failure')
    .map(([name]) => ({ job: name, step: null }))
}

export function preflightReason(needs) {
  for (const job of Object.values(needs ?? {})) {
    const reason = job?.outputs?.notary_preflight_reason
    if (reason) return reason
  }
  return ''
}

export function renderAlert({ reason, workflow, runUrl, trigger, failures }) {
  const lines = []
  if (reason) lines.push(reason, '')
  lines.push(
    `- Workflow: ${workflow}`,
    `- Run: ${runUrl}`,
    `- Trigger: ${trigger}`,
    '- Failed:',
  )
  if (failures.length) {
    for (const { job, step } of failures) {
      lines.push(step ? `  - \`${job}\` / \`${step}\`` : `  - \`${job}\``)
    }
  } else {
    lines.push('  - (failed job could not be determined)')
  }
  return lines.join('\n')
}

export function renderRecovered(runUrl) {
  return `Recovered: ${runUrl}`
}

/**
 * openIssues 为同标题且带 ci-failure 标签的未关闭 Issue，按编号升序。
 * 失败：已有则在最早一条追加评论，否则新建；成功：逐条评论恢复并关闭。
 */
export function planAlert(outcome, openIssues) {
  if (outcome === 'failure') {
    return openIssues.length
      ? { action: 'comment', issues: [openIssues[0]] }
      : { action: 'create', issues: [] }
  }
  if (outcome === 'success' && openIssues.length) {
    return { action: 'close', issues: openIssues }
  }
  return { action: 'none', issues: [] }
}

export function matchingOpenIssues(issues, title) {
  return issues
    .filter(issue => !issue.pull_request && issue.title === title)
    .sort((left, right) => left.number - right.number)
}

function runGh(args) {
  return execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
}

function ensureLabel(gh, repo) {
  try {
    gh(['api', `repos/${repo}/labels/${ALERT_LABEL}`])
  } catch (error) {
    if (!/404|Not Found/i.test(String(error.stderr ?? error.message))) throw error
    gh([
      'api', '-X', 'POST', `repos/${repo}/labels`,
      '-f', `name=${ALERT_LABEL}`,
      '-f', `color=${LABEL_COLOR}`,
      '-f', `description=${LABEL_DESCRIPTION}`,
    ])
  }
}

function collectFailures(gh, env, needs) {
  try {
    const jobs = JSON.parse(gh([
      'api',
      `repos/${env.GITHUB_REPOSITORY}/actions/runs/${env.GITHUB_RUN_ID}/attempts/${env.GITHUB_RUN_ATTEMPT || 1}/jobs?per_page=100`,
      '--jq', '.jobs',
    ]))
    const failures = failuresFromJobs(jobs, env.ALERT_JOB_NAME)
    if (failures.length) return failures
  } catch {
    // 读不到步骤明细时退回 needs 结果，告警照常送达
  }
  return failuresFromNeeds(needs)
}

function summary(env, text) {
  console.log(text)
  if (env.GITHUB_STEP_SUMMARY) appendFileSync(env.GITHUB_STEP_SUMMARY, `${text}\n`)
}

/** gh 可注入：参数数组 → stdout，失败时抛出带 stderr 的错误。 */
export function main(env, gh) {
  const needs = JSON.parse(env.NEEDS_JSON || '{}')
  const workflow = env.GITHUB_WORKFLOW
  const repo = env.GITHUB_REPOSITORY
  const runUrl = `${env.GITHUB_SERVER_URL}/${repo}/actions/runs/${env.GITHUB_RUN_ID}`
  const title = issueTitle(workflow)
  const outcome = runOutcome(needs)
  if (outcome === 'none') {
    summary(env, `CI failure alert: no action (run neither failed nor fully succeeded).`)
    return
  }

  const openIssues = matchingOpenIssues(gh([
    'api', '--paginate',
    `repos/${repo}/issues?state=open&labels=${ALERT_LABEL}&per_page=100`,
    '--jq', '.[] | {number, title, pull_request}',
  ]).split('\n').filter(Boolean).map(line => JSON.parse(line)), title)
  const plan = planAlert(outcome, openIssues)

  if (plan.action === 'create' || plan.action === 'comment') {
    const body = renderAlert({
      reason: preflightReason(needs),
      workflow,
      runUrl,
      trigger: describeTrigger({
        eventName: env.GITHUB_EVENT_NAME,
        refType: env.GITHUB_REF_TYPE,
        refName: env.GITHUB_REF_NAME,
      }),
      failures: collectFailures(gh, env, needs),
    })
    if (plan.action === 'create') {
      ensureLabel(gh, repo)
      const url = gh([
        'api', '-X', 'POST', `repos/${repo}/issues`,
        '-f', `title=${title}`,
        '-f', `body=${body}`,
        '-f', `labels[]=${ALERT_LABEL}`,
        '--jq', '.html_url',
      ]).trim()
      summary(env, `CI failure alert: opened ${url}`)
    } else {
      const [issue] = plan.issues
      gh(['api', '-X', 'POST', `repos/${repo}/issues/${issue.number}/comments`, '-f', `body=${body}`])
      summary(env, `CI failure alert: commented on #${issue.number}`)
    }
    return
  }

  if (plan.action === 'close') {
    for (const issue of plan.issues) {
      gh(['api', '-X', 'POST', `repos/${repo}/issues/${issue.number}/comments`, '-f', `body=${renderRecovered(runUrl)}`])
      gh(['api', '-X', 'PATCH', `repos/${repo}/issues/${issue.number}`, '-f', 'state=closed', '-f', 'state_reason=completed'])
      summary(env, `CI failure alert: recovered and closed #${issue.number}`)
    }
    return
  }

  summary(env, 'CI failure alert: no open alert to close.')
}

/** 告警失败只记入运行摘要并正常返回，不让本 job 失败，避免改写原 workflow 的结论。 */
export function reportCiStatus(env = process.env, gh = runGh) {
  try {
    main(env, gh)
    return true
  } catch (error) {
    const detail = String(error?.stderr || error?.message || error).trim().split('\n')[0]
    summary(env, `CI failure alert could not be delivered: ${detail}`)
    return false
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  reportCiStatus()
}
