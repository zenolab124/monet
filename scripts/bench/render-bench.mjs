/**
 * P1 会话性能专项 — 渲染管线微基准
 *
 * 运行: node --expose-gc scripts/bench/render-bench.mjs
 * (Node >= 23 原生 type stripping，可直接 import 项目 src 下的 .ts 模块)
 *
 * 保真度策略:
 * - s2/s3 直接 import 项目真实的 renderMarkdownPlain（流式热路径用的就是它）
 * - shiki 路径用「复刻实例」（与 useMarkdown.ts 完全相同的 mdOpts/LANGS/themes 配置），
 *   并在启动时与真实模块的 renderMarkdownCached 输出做字符串相等断言，证明零偏差。
 *   不直接用 renderMarkdownCached 计时是因为它带 LRU 缓存，会污染"冷渲染"测量。
 * - s4 复刻 parser.rs 的字符串预过滤（file-history-snapshot/queue-operation/ai-title）
 *
 * 禁改 src/ 与 src-tauri/；~/.claude/projects 只读。
 */

import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { execSync } from 'node:child_process'
import MarkdownIt from 'markdown-it'
import markdownItShiki from '@shikijs/markdown-it'
// 真实渲染模块（import 即触发其内部 shiki 异步初始化）
import { renderMarkdownPlain, renderMarkdownCached } from '../../src/composables/useMarkdown.ts'

// ─────────────────────────────────────────────
// 复刻 useMarkdown.ts 的实例配置（逐字一致）
// ─────────────────────────────────────────────
const LANGS = [
  'javascript', 'typescript', 'python', 'rust', 'go', 'java',
  'bash', 'shell', 'json', 'yaml', 'toml', 'html', 'css',
  'vue', 'jsx', 'tsx', 'sql', 'swift', 'kotlin', 'ruby',
  'c', 'cpp', 'diff', 'markdown', 'xml',
]
const mdOpts = { html: false, linkify: true, breaks: false, typographer: false }

const replicaPlain = new MarkdownIt(mdOpts)
const shikiPlugin = await markdownItShiki({
  themes: { light: 'github-light', dark: 'github-dark' },
  langs: LANGS,
  defaultColor: false,
})
const replicaShiki = new MarkdownIt(mdOpts)
replicaShiki.use(shikiPlugin)

// 等待真实模块内部的 shiki 就绪（它不导出 ready 标志，轮询探测输出特征）
async function waitRealShikiReady() {
  for (let i = 0; i < 200; i++) {
    const probe = '```js\nlet probe' + i + ' = 1\n```\n'
    if (renderMarkdownCached(probe).includes('class="shiki')) return
    await new Promise(r => setTimeout(r, 50))
  }
  throw new Error('real module shiki not ready after 10s')
}

// ─────────────────────────────────────────────
// 工具
// ─────────────────────────────────────────────
function gc() { globalThis.gc?.() }

/** 多轮取中位数（含 min/max） */
function bench(fn, rounds) {
  const times = []
  for (let i = 0; i < rounds; i++) {
    gc()
    const t0 = performance.now()
    fn(i)
    times.push(performance.now() - t0)
  }
  const sorted = [...times].sort((a, b) => a - b)
  return {
    median: sorted[(sorted.length - 1) >> 1],
    min: sorted[0],
    max: sorted[sorted.length - 1],
    rounds,
  }
}

const fmt = (ms) => ms >= 100 ? ms.toFixed(0) : ms >= 10 ? ms.toFixed(1) : ms.toFixed(2)

// ─────────────────────────────────────────────
// 典型 markdown 文档生成器（确定性）
// ─────────────────────────────────────────────
const PARA = '在长会话渲染管线中，markdown 解析成本会随文本长度增长，This paragraph mixes Chinese and English to mimic real assistant output, with `inline code`, **bold emphasis**, *italics* and [a link](https://example.com/docs)。渲染器需要处理混合内容的分词、内联解析与 linkify 探测 https://auto.link/path 等自动链接场景。'
const LIST = '- 第一项：解析 token 流并构建块级结构\n- 第二项：内联解析处理 `inline code` 与 **强调**\n- 第三项：表格与代码块的边界检测逻辑\n- 第四项：linkify 对 www.example.com 的探测\n- 第五项：输出 HTML 字符串并交给 v-html'
const TABLE = '| 阶段 | 耗时 | 备注 |\n| --- | --- | --- |\n| parse | 1.2ms | markdown-it 块级+内联 |\n| highlight | 8.4ms | shiki 双主题 |\n| patch | 0.6ms | v-html 整块替换 |'

const CODE_SNIPPETS = [
  ['javascript', (n) => Array.from({ length: n }, (_, i) =>
    `const value${i} = compute(${i}) * factor; // 计算第 ${i} 项`).join('\n')],
  ['python', (n) => Array.from({ length: n }, (_, i) =>
    `result_${i} = process(items[${i}], key=lambda x: x.weight)  # 处理第 ${i} 项`).join('\n')],
  ['rust', (n) => Array.from({ length: n }, (_, i) =>
    `let record_${i}: Vec<SessionRecord> = parse_messages(&path_${i})?; // 解析`).join('\n')],
  ['typescript', (n) => Array.from({ length: n }, (_, i) =>
    `const handler${i}: (e: Event) => void = (e) => dispatch(${i}, e.type)`).join('\n')],
  ['bash', (n) => Array.from({ length: n }, (_, i) =>
    `grep -rn "pattern_${i}" src/ | head -${i + 1} # 搜索第 ${i} 处`).join('\n')],
]

/** 生成约 targetChars 字符的混合文档，含 codeBlocks 个代码块（每个 codeLines 行），按长度进度均匀插入 */
function makeDoc(targetChars, codeBlocks = 4, codeLines = 15) {
  const fillers = [PARA, LIST, PARA, TABLE]
  const parts = []
  let len = 0
  let fi = 0, ci = 0, sectionN = 0, blockCount = 0
  while (len < targetChars) {
    if (blockCount % 7 === 0) {
      const h = `## 第 ${++sectionN} 节：渲染管线分析`
      parts.push(h); len += h.length + 2
    }
    // 长度进度驱动的代码块插入：第 i 个代码块在文档 (i+1)/(blocks+1) 处出现
    if (ci < codeBlocks && len >= ((ci + 1) / (codeBlocks + 1)) * targetChars) {
      const [lang, gen] = CODE_SNIPPETS[ci % CODE_SNIPPETS.length]
      const code = '```' + lang + '\n' + gen(codeLines) + '\n```'
      parts.push(code); len += code.length + 2; ci++
    }
    const f = fillers[fi++ % fillers.length]
    parts.push(f); len += f.length + 2
    blockCount++
  }
  return parts.join('\n\n')
}

// ─────────────────────────────────────────────
// 保真度断言
// ─────────────────────────────────────────────
async function verifyFidelity() {
  await waitRealShikiReady()
  const sample = makeDoc(3000)
  const okPlain = replicaPlain.render(sample) === renderMarkdownPlain(sample)
  const okShiki = replicaShiki.render(sample) === renderMarkdownCached(sample)
  console.log(`[fidelity] replicaPlain === renderMarkdownPlain: ${okPlain}`)
  console.log(`[fidelity] replicaShiki === renderMarkdownCached: ${okShiki}`)
  if (!okPlain || !okShiki) throw new Error('复刻实例输出与真实模块不一致，基准无效')
}

// ─────────────────────────────────────────────
// s0: JIT 冷/热对照 — 同一 64k 文档连续渲染，观察引擎优化带来的耗时漂移
// （解释 s1 中位数与 s2 稳态 flush 耗时的差异来源）
// ─────────────────────────────────────────────
function s0() {
  console.log('\n══════ s0: JIT 冷/热对照（同一 64k 文档连续 plain render 30 次） ══════')
  const doc = makeDoc(65536)
  const times = []
  for (let i = 0; i < 30; i++) {
    const t0 = performance.now()
    renderMarkdownPlain(doc)
    times.push(performance.now() - t0)
  }
  const pick = [0, 1, 2, 4, 9, 29]
  console.log(pick.map(i => `run${i + 1}: ${fmt(times[i])}ms`).join(' | '))
  console.log('（注意：s1 各尺寸轮数少、偏冷态；s2 流式 656 次 flush 为热态——真实 app 流式期同样是热态）')
  return { coldMs: times[0], warmMs: times[29] }
}

// ─────────────────────────────────────────────
// s1: 单次 render 耗时 vs 文本长度
// ─────────────────────────────────────────────
function s1() {
  console.log('\n══════ s1: 单次 markdown render 耗时 vs 文本长度 ══════')
  const sizes = [1024, 4096, 16384, 65536, 262144]
  const roundsBySize = { 1024: 21, 4096: 21, 16384: 11, 65536: 7, 262144: 5 }
  // 小文档按比例缩小代码块规模，避免代码块本身撑爆目标长度
  const codeCfg = { 1024: [3, 3], 4096: [4, 8], 16384: [4, 15], 65536: [4, 15], 262144: [5, 15] }
  const rows = []
  for (const size of sizes) {
    const doc = makeDoc(size, ...codeCfg[size])
    const rounds = roundsBySize[size]
    // 预热
    replicaPlain.render(doc); replicaShiki.render(doc)
    const plain = bench(() => replicaPlain.render(doc), rounds)
    const shiki = bench(() => replicaShiki.render(doc), rounds)
    // LRU 命中成本参照（真实 renderMarkdownCached 第二次调用）
    renderMarkdownCached(doc)
    const hit = bench(() => renderMarkdownCached(doc), rounds)
    rows.push({ size, actualLen: doc.length, plain, shiki, hit })
    console.log(
      `${(size / 1024).toFixed(0).padStart(4)}k (实际 ${doc.length} 字符): ` +
      `plain ${fmt(plain.median)}ms | shiki ${fmt(shiki.median)}ms | LRU命中 ${fmt(hit.median)}ms` +
      `  (median of ${rounds})`)
  }
  return rows
}

// ─────────────────────────────────────────────
// s2: 模拟流式累积 — 每 ~100 字符 flush，全文重 parse（真实 renderMarkdownPlain）
// ─────────────────────────────────────────────
const FLUSH_CHARS = 100
const TRUNCATE_LEN = 8192 // BlockText.vue TEXT_TRUNCATE_LEN

function runStreamFullReparse(doc) {
  const flushTimes = []
  for (let pos = FLUSH_CHARS; ; pos += FLUSH_CHARS) {
    const end = Math.min(pos, doc.length)
    const slice = doc.slice(0, end)
    const t0 = performance.now()
    renderMarkdownPlain(slice)
    flushTimes.push(performance.now() - t0)
    if (end === doc.length) break
  }
  return flushTimes
}

function s2() {
  console.log('\n══════ s2: 流式累积 0→64k，每 100 字符 flush 全文重 parse ══════')
  const doc = makeDoc(65536)
  console.log(`文档实际 ${doc.length} 字符，共 ${Math.ceil(doc.length / FLUSH_CHARS)} 次 flush`)
  // 跑 3 遍，取总耗时中位的那一遍
  const runs = []
  for (let r = 0; r < 3; r++) { gc(); runs.push(runStreamFullReparse(doc)) }
  const totals = runs.map(ts => ts.reduce((a, b) => a + b, 0))
  const midIdx = totals.indexOf([...totals].sort((a, b) => a - b)[1])
  const ts = runs[midIdx]
  const total = totals[midIdx]
  const n = ts.length

  // 进度百分位处的单次 flush 耗时（取该位置前后 5 次的均值降噪）
  const at = (p) => {
    const idx = Math.min(n - 1, Math.round((p / 100) * (n - 1)))
    const lo = Math.max(0, idx - 2), hi = Math.min(n, idx + 3)
    const w = ts.slice(lo, hi)
    return w.reduce((a, b) => a + b, 0) / w.length
  }
  const pcts = [1, 25, 50, 75, 100].map(p => ({ p, ms: at(p) }))
  console.log(`总累积 parse 耗时: ${fmt(total)}ms  (3 次运行: ${totals.map(fmt).join(' / ')}ms)`)
  console.log('进度百分位处单次 flush 耗时: ' +
    pcts.map(({ p, ms }) => `${p}% → ${fmt(ms)}ms`).join(' | '))
  const ratio = at(100) / at(25)
  console.log(`t(100%)/t(25%) = ${ratio.toFixed(2)}  (严格线性增长即 O(n²) 应≈4)`)

  // 对照：app 真实行为 — BlockText 8K 截断后 computed 冻结，8192 字符后不再 parse
  const frozenFlushes = Math.ceil(TRUNCATE_LEN / FLUSH_CHARS)
  const appTotal = ts.slice(0, frozenFlushes).reduce((a, b) => a + b, 0)
  console.log(`[app 真实行为对照] 8K 截断冻结后实际只有前 ${frozenFlushes} 次 flush 触发 parse，` +
    `累积 ${fmt(appTotal)}ms（其后 displayText 不变，computed 不重算）`)
  return { docLen: doc.length, flushes: n, totalMs: total, pcts, ratio, appTotalMs: appTotal, frozenFlushes }
}

// ─────────────────────────────────────────────
// s3: 对照组 — 稳定段落缓存 + 活跃尾部增量（候选手段 a 的粗略模拟）
// ─────────────────────────────────────────────
function runStreamSegmented(doc) {
  const flushTimes = []
  let stableEnd = 0   // doc[0..stableEnd) 已固化（保证在段边界且 fence 配平）
  let stableHtml = ''
  for (let pos = FLUSH_CHARS; ; pos += FLUSH_CHARS) {
    const end = Math.min(pos, doc.length)
    const t0 = performance.now()
    const tail = doc.slice(stableEnd, end)
    // 找尾部中最后一个安全分段点：最后一个 "\n\n" 且其左侧 fence 数为偶（不在代码块内）
    const lastBreak = tail.lastIndexOf('\n\n')
    if (lastBreak > 0) {
      const closing = tail.slice(0, lastBreak)
      const fences = (closing.match(/```/g) || []).length
      if (fences % 2 === 0) {
        // 固化新闭合的段（一次性 parse，之后不再重 parse）
        stableHtml += renderMarkdownPlain(closing)
        stableEnd += lastBreak + 2
      }
    }
    // 每次 flush 只重 parse 活跃尾部
    const activeTail = doc.slice(stableEnd, end)
    const html = stableHtml + renderMarkdownPlain(activeTail)
    flushTimes.push(performance.now() - t0)
    if (end === doc.length) { void html; break }
  }
  return flushTimes
}

function s3(s2Result) {
  console.log('\n══════ s3: 对照组 — 稳定段落缓存 + 活跃尾部增量 parse ══════')
  const doc = makeDoc(65536)
  const runs = []
  for (let r = 0; r < 3; r++) { gc(); runs.push(runStreamSegmented(doc)) }
  const totals = runs.map(ts => ts.reduce((a, b) => a + b, 0))
  const total = [...totals].sort((a, b) => a - b)[1]
  const ts = runs[totals.indexOf(total)]
  const maxFlush = Math.max(...ts)
  console.log(`总累积耗时: ${fmt(total)}ms  (3 次运行: ${totals.map(fmt).join(' / ')}ms)，` +
    `最慢单次 flush ${fmt(maxFlush)}ms`)
  console.log(`vs s2 全文重 parse ${fmt(s2Result.totalMs)}ms → 提速 ${(s2Result.totalMs / total).toFixed(1)}x`)
  console.log(`vs s2 的 app 真实(8K 冻结) ${fmt(s2Result.appTotalMs)}ms → ${(s2Result.appTotalMs / total).toFixed(1)}x`)
  return { totalMs: total, maxFlushMs: maxFlush, speedupVsS2: s2Result.totalMs / total }
}

// ─────────────────────────────────────────────
// s4: 真实大会话冷渲染（数据层 → HTML 的 CPU 成本，不含 DOM）
// ─────────────────────────────────────────────
function findTop3Jsonl() {
  const root = path.join(os.homedir(), '.claude', 'projects')
  // 用 wc -l 找最大 3 个（只读）
  const out = execSync(
    `find ${JSON.stringify(root)} -name '*.jsonl' -type f -print0 | xargs -0 wc -l | sort -rn | grep -v ' total$' | head -3`,
    { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 })
  return out.trim().split('\n').map(l => {
    const m = l.trim().match(/^(\d+)\s+(.+)$/)
    return { lines: Number(m[1]), file: m[2] }
  })
}

function s4() {
  console.log('\n══════ s4: 真实大会话冷渲染（JSON parse + markdown，不含 DOM/IPC） ══════')
  const files = findTop3Jsonl()
  const results = []
  for (const { file, lines } of files) {
    gc()
    const t0 = performance.now()
    const raw = fs.readFileSync(file, 'utf8')
    const readMs = performance.now() - t0
    const fileLines = raw.split('\n')

    // 复刻 parser.rs 的预过滤 + 逐行 JSON parse
    const t1 = performance.now()
    const records = []
    for (const line of fileLines) {
      if (!line.trim()) continue
      if (line.includes('"file-history-snapshot"') ||
          line.includes('"queue-operation"') ||
          line.includes('"ai-title"')) continue
      try {
        const v = JSON.parse(line)
        if (v.type === 'user' || v.type === 'assistant') records.push(v)
      } catch { /* 跳过坏行 */ }
    }
    const jsonMs = performance.now() - t1

    // 提取全部 assistant text 块
    const texts = []
    for (const r of records) {
      if (r.type !== 'assistant') continue
      const c = r.message?.content
      if (Array.isArray(c)) {
        for (const b of c) if (b.type === 'text' && typeof b.text === 'string' && b.text) texts.push(b.text)
      }
    }
    const totalChars = texts.reduce((a, t) => a + t.length, 0)

    // markdown 渲染：全文 shiki（理论冷成本）
    // 注意：未配置 fallbackLanguage 时，LANGS 之外的语言会让 shiki 抛 ShikiError——
    // 真实 app 的 renderMarkdownCached 同样会抛（已在基准外验证），这里 catch 并计数
    const unknownLangs = new Set()
    const renderSafe = (md, t) => {
      try { md.render(t); return true }
      catch (e) {
        const m = String(e.message).match(/Language `(.+?)` not found/)
        if (m) unknownLangs.add(m[1])
        return false
      }
    }
    gc()
    let shikiErrors = 0
    const t2 = performance.now()
    for (const t of texts) { if (!renderSafe(replicaShiki, t)) shikiErrors++ }
    const mdShikiFullMs = performance.now() - t2

    // app 真实路径：BlockText 截断到 8192 字符后再渲染
    gc()
    const t3 = performance.now()
    for (const t of texts) renderSafe(replicaShiki, t.length > TRUNCATE_LEN ? t.slice(0, TRUNCATE_LEN) : t)
    const mdShikiTruncMs = performance.now() - t3

    // 参照：plain（无 shiki）截断版
    gc()
    const t4 = performance.now()
    for (const t of texts) replicaPlain.render(t.length > TRUNCATE_LEN ? t.slice(0, TRUNCATE_LEN) : t)
    const mdPlainTruncMs = performance.now() - t4

    const sizeMB = (raw.length / 1024 / 1024).toFixed(1)
    console.log(`\n${path.basename(path.dirname(file))}/${path.basename(file)}`)
    console.log(`  ${lines} 行 / ${sizeMB}MB / ${records.length} 条 user+assistant record / ` +
      `${texts.length} 个 text 块 / 文本共 ${(totalChars / 1024).toFixed(0)}k 字符`)
    console.log(`  文件读取 ${fmt(readMs)}ms | JSON parse ${fmt(jsonMs)}ms | ` +
      `markdown+shiki(全文) ${fmt(mdShikiFullMs)}ms | markdown+shiki(8K 截断=app 实况) ${fmt(mdShikiTruncMs)}ms | ` +
      `plain(8K 截断) ${fmt(mdPlainTruncMs)}ms`)
    if (shikiErrors > 0) {
      console.log(`  ⚠ ${shikiErrors} 个 text 块因未加载语言抛 ShikiError（真实 app 同样会抛）: ${[...unknownLangs].join(', ')}`)
    }
    results.push({
      file, lines, sizeMB: Number(sizeMB), records: records.length,
      textBlocks: texts.length, totalChars, readMs, jsonMs,
      mdShikiFullMs, mdShikiTruncMs, mdPlainTruncMs,
      shikiErrors, unknownLangs: [...unknownLangs],
    })
  }
  return results
}

// ─────────────────────────────────────────────
// s5: shiki 高亮单独计价 — 30 个代码块 × ~50 行
// ─────────────────────────────────────────────
function s5() {
  console.log('\n══════ s5: shiki 高亮单独计价（30 代码块 × 50 行） ══════')
  const parts = []
  for (let i = 0; i < 30; i++) {
    const [lang, gen] = CODE_SNIPPETS[i % CODE_SNIPPETS.length]
    parts.push(`第 ${i + 1} 个代码块（${lang}）的说明段落，模拟 assistant 输出的过渡文本。`)
    parts.push('```' + lang + '\n' + gen(50) + '\n```')
  }
  const doc = parts.join('\n\n')
  console.log(`文档 ${doc.length} 字符，30 个代码块（5 种语言轮换，各 50 行）`)
  // 预热
  replicaPlain.render(doc); replicaShiki.render(doc)
  const plain = bench(() => replicaPlain.render(doc), 7)
  const shiki = bench(() => replicaShiki.render(doc), 7)
  const delta = shiki.median - plain.median
  console.log(`markdown-it 无高亮: ${fmt(plain.median)}ms | +shiki 同步高亮: ${fmt(shiki.median)}ms | ` +
    `差值 ${fmt(delta)}ms (${(shiki.median / plain.median).toFixed(0)}x) | 摊到单块 ≈ ${fmt(delta / 30)}ms`)
  return { docLen: doc.length, plainMs: plain.median, shikiMs: shiki.median, deltaMs: delta, perBlockMs: delta / 30 }
}

// ─────────────────────────────────────────────
// main
// ─────────────────────────────────────────────
console.log(`Node ${process.version} | ${os.cpus()[0]?.model ?? 'unknown CPU'} | gc 暴露: ${!!globalThis.gc}`)
await verifyFidelity()
const r0 = s0()
const r1 = s1()
const r2 = s2()
const r3 = s3(r2)
const r4 = s4()
const r5 = s5()
console.log('\n══════ JSON 结果 ══════')
console.log(JSON.stringify({ s0: r0, s1: r1, s2: r2, s3: r3, s4: r4, s5: r5 }, (k, v) =>
  typeof v === 'number' ? Math.round(v * 1000) / 1000 : v, 1))
