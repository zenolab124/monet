import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createStreamSplitter } from '../../src/lib/stream-markdown/findSafeSplit'
import { normalizeHtml } from '../../src/lib/stream-markdown/devConsistencyCheck'
import { renderMarkdownPlain } from '../../src/composables/useMarkdown'

// 金样快照测试(PRD v2.5.0 FR-007):真实会话长文本样本上断言
// 「逐段渲染拼接」与「全文一次渲染」归一化后语义等价——段组件化正确性的最终防线
const DIR = fileURLToPath(new URL('../fixtures/streaming-samples', import.meta.url))

interface IndexEntry {
  file: string
  chars: number
  features: string[]
  synthetic: boolean
}
const index: IndexEntry[] = JSON.parse(readFileSync(join(DIR, 'index.json'), 'utf8'))
const readSample = (e: IndexEntry) => readFileSync(join(DIR, e.file), 'utf8')

/**
 * 镜像 useStreamSegments.ingest 的冻结规则(间隔 ≥MIN_SEGMENT_LEN 才成段)。
 * 若改动 useStreamSegments 的切段策略,此处须同步——两处漂移会让金样失去意义。
 */
const MIN_SEGMENT_LEN = 200
function segmentize(text: string): { segs: string[]; blocked: boolean } {
  const sp = createStreamSplitter()
  const points = sp.update(text)
  if (sp.blocked) return { segs: [text], blocked: true }
  const segs: string[] = []
  let start = 0
  for (const p of points) {
    if (p <= start || p - start < MIN_SEGMENT_LEN) continue
    segs.push(text.slice(start, p))
    start = p
  }
  segs.push(text.slice(start)) // tail
  return { segs, blocked: false }
}

/** LCG 伪随机流式追加(可复现),返回最终分割点 */
function incrementalPoints(text: string, seed: number): number[] {
  let s = seed
  const rand = () => (s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff
  const sp = createStreamSplitter()
  let fed = 0
  let pts: number[] = []
  while (fed < text.length) {
    fed = Math.min(text.length, fed + 1 + Math.floor(rand() * 50))
    pts = sp.update(text.slice(0, fed))
  }
  return pts
}

describe('金样:逐段拼接 = 全文渲染', () => {
  for (const entry of index) {
    it(`${entry.file} (${entry.chars} chars) [${entry.features.join(',')}]`, () => {
      const text = readSample(entry)
      const { segs } = segmentize(text)
      const joined = segs.map(renderMarkdownPlain).join('')
      const full = renderMarkdownPlain(text)
      expect(normalizeHtml(joined)).toBe(normalizeHtml(full))
    })
  }
})

describe('金样:守卫行为', () => {
  it('脚注/引用式链接样本触发整块封锁', () => {
    const guarded = index.filter(e => e.features.includes('footnote') || e.features.includes('reflink'))
    expect(guarded.length).toBeGreaterThanOrEqual(2)
    for (const entry of guarded) {
      const { blocked, segs } = segmentize(readSample(entry))
      expect(blocked, entry.file).toBe(true)
      expect(segs.length, entry.file).toBe(1)
    }
  })

  it('无守卫特征的真实长样本应产生多段(分段机制未被守卫误杀)', () => {
    const plain = index.filter(e => !e.synthetic
      && !e.features.includes('footnote') && !e.features.includes('reflink')
      && !e.features.includes('inline_html') && !e.features.includes('math'))
    expect(plain.length).toBeGreaterThanOrEqual(5)
    const multi = plain.filter(e => segmentize(readSample(e)).segs.length > 1)
    // 真实长文本大多含普通段落间隔:至少一半样本应可分段
    expect(multi.length).toBeGreaterThanOrEqual(Math.ceil(plain.length / 2))
  })
})

describe('金样:增量契约', () => {
  it('随机步长流式追加与一次性全文分割点一致', () => {
    for (const entry of index) {
      const text = readSample(entry)
      const full = createStreamSplitter().update(text)
      for (const seed of [3, 97]) {
        expect(incrementalPoints(text, seed), `${entry.file} seed=${seed}`).toEqual(full)
      }
    }
  })
})
