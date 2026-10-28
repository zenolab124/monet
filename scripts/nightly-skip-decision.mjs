#!/usr/bin/env node
// Nightly 定时触发的跳过判定：只有 HEAD 已经作为当前 Nightly 发布（与 nightly
// tag 同一提交）才跳过。上次构建失败时 tag 仍停在旧提交，次日会自动重试。
// 判定依据无论跳过与否都写入运行摘要。
import { execFileSync } from 'node:child_process'
import { appendFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const short = sha => sha.slice(0, 7)

/**
 * 读取 nightly tag 指向的提交。runGit 接收参数数组、返回 stdout，失败时抛错。
 * 返回 { state: 'present', sha } / { state: 'missing' } / { state: 'error', error }。
 */
export function readNightlyTag(runGit) {
  try {
    if (!runGit(['tag', '--list', 'nightly']).trim()) return { state: 'missing' }
    return { state: 'present', sha: runGit(['rev-parse', 'nightly^{commit}']).trim() }
  } catch (error) {
    const detail = String(error?.stderr || error?.message || error).trim().split('\n')[0]
    return { state: 'error', error: detail || 'unknown error' }
  }
}

/**
 * eventName：触发事件；headSha：当前提交；tag：readNightlyTag 的结果；
 * release：当前 Nightly 的 { version, publishedAt }，仅在需要写跳过摘要时读取。
 */
export function decideNightlySkip({ eventName, headSha, tag, release }) {
  if (eventName !== 'schedule') {
    return { skip: false, summary: `Triggered by ${eventName || 'unknown event'}; building.` }
  }
  if (tag.state === 'missing') {
    return { skip: false, summary: 'No published Nightly found; building.' }
  }
  if (tag.state === 'error') {
    return { skip: false, summary: `Could not read the nightly tag (${tag.error}); building.` }
  }
  if (tag.sha === headSha) {
    const version = release?.version || 'unknown version'
    const publishedAt = release?.publishedAt || 'unknown time'
    return {
      skip: true,
      summary: `Skipped: HEAD ${short(headSha)} is already published as Nightly ${version} (${publishedAt})`,
    }
  }
  return {
    skip: false,
    summary: `HEAD ${short(headSha)} differs from the published Nightly at ${short(tag.sha)}; building.`,
  }
}

/** Release 标题形如 "Nightly 1.2.3"；读不到时摘要退回 unknown，不影响判定。 */
export function parseNightlyRelease(payload) {
  const name = String(payload?.name ?? '')
  return {
    version: name.replace(/^Nightly\s+/i, '') || '',
    publishedAt: payload?.published_at ?? '',
  }
}

const run = command => args => execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })

/** git / gh 可注入，便于在不访问 GitHub 的情况下测试完整流程。 */
export function main(env = process.env, { runGit = run('git'), runGh = run('gh') } = {}) {
  const eventName = env.EVENT_NAME || env.GITHUB_EVENT_NAME
  const headSha = runGit(['rev-parse', 'HEAD']).trim()
  const tag = eventName === 'schedule' ? readNightlyTag(runGit) : { state: 'missing' }

  let release = null
  if (eventName === 'schedule' && tag.state === 'present' && tag.sha === headSha) {
    try {
      release = parseNightlyRelease(JSON.parse(runGh([
        'api', `repos/${env.GITHUB_REPOSITORY}/releases/tags/nightly`, '--jq', '{name, published_at}',
      ])))
    } catch {
      // 版本与时间只用于摘要；判定只认 tag 指向的提交
    }
  }

  const decision = decideNightlySkip({ eventName, headSha, tag, release })
  console.log(decision.summary)
  if (env.GITHUB_OUTPUT) appendFileSync(env.GITHUB_OUTPUT, `skip=${decision.skip}\n`)
  if (env.GITHUB_STEP_SUMMARY) appendFileSync(env.GITHUB_STEP_SUMMARY, `${decision.summary}\n`)
  return decision
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main()
}
