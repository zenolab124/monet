/**
 * dev-only 性能埋点（P1 会话性能专项 · 运行时基线采集）。
 * 输出全部走 console（WebView devtools），生产构建下所有入口为空操作。
 *
 * 覆盖四个静态诊断回答不了的缺口：
 * 1. 会话加载四段分解：invoke(Rust parse+IPC) / Vue patch+mount / 首帧 layout+paint / markdown 占比
 * 2. 流式期帧间隔分布与掉帧率（多路并行时记并发峰值）
 * 3. 流式结束翻转帧耗时（素色→shiki 一次性上色的主线程阻塞）
 * 4. markdown LRU 命中率（首开 vs 回访的差异来源）
 */
import { nextTick } from 'vue'

// ?. 兼容 node 直跑(bench 脚本经 useMarkdown 引到本模块,node 下 import.meta.env 为 undefined)
const enabled = !!import.meta.env?.DEV

const TAG = '%c[perf]'
const TAG_STYLE = 'color:#b45309;font-weight:bold'

// ---- markdown 渲染计量（useMarkdown 上报，累计值；各报告取差值） ----

interface MdCounters {
  plainMs: number
  plainN: number
  renderMs: number // cached miss 的真实渲染耗时（shiki 在内）
  hit: number
  miss: number
}
const md: MdCounters = { plainMs: 0, plainN: 0, renderMs: 0, hit: 0, miss: 0 }

export function probeMd(kind: 'plain' | 'hit' | 'miss', ms: number) {
  if (!enabled) return
  if (kind === 'plain') {
    md.plainMs += ms
    md.plainN++
  } else if (kind === 'hit') {
    md.hit++
  } else {
    md.renderMs += ms
    md.miss++
  }
}

function mdSnap(): MdCounters {
  return { ...md }
}

function mdDiff(prev: MdCounters): MdCounters {
  return {
    plainMs: md.plainMs - prev.plainMs,
    plainN: md.plainN - prev.plainN,
    renderMs: md.renderMs - prev.renderMs,
    hit: md.hit - prev.hit,
    miss: md.miss - prev.miss,
  }
}

const fmt = (ms: number) => `${ms.toFixed(1)}ms`

function nextFrame(): Promise<number> {
  return new Promise(resolve => requestAnimationFrame(() => resolve(performance.now())))
}

// ---- 1. 会话加载四段分解 ----

export interface SessionLoadProbe {
  afterInvoke(recordCount: number): void
  /** records 赋值后调用；内部 nextTick+双 rAF 异步收尾，不阻塞调用方 */
  afterAssign(): void
}

export function probeSessionLoad(sessionId: string): SessionLoadProbe | null {
  if (!enabled) return null
  const t0 = performance.now()
  let tInvoke = t0
  let count = 0
  return {
    afterInvoke(recordCount: number) {
      tInvoke = performance.now()
      count = recordCount
    },
    afterAssign() {
      const mdBefore = mdSnap()
      void (async () => {
        await nextTick() // Vue patch + 子组件 mount（markdown 同步渲染在其中）
        const tPatch = performance.now()
        const d = mdDiff(mdBefore)
        await nextFrame()
        await nextFrame()
        const tPaint = performance.now()
        console.log(
          `${TAG} 会话加载 ${sessionId.slice(0, 8)} · ${count} records\n` +
            `  invoke(Rust parse+IPC): ${fmt(tInvoke - t0)}\n` +
            `  Vue patch+mount:        ${fmt(tPatch - tInvoke)}（markdown ${fmt(d.renderMs + d.plainMs)} · LRU ${d.hit} hit / ${d.miss} miss）\n` +
            `  首帧 layout+paint:      ${fmt(tPaint - tPatch)}\n` +
            `  合计:                   ${fmt(tPaint - t0)}`,
          TAG_STYLE,
        )
      })()
    },
  }
}

// ---- 2. 流式期帧间隔监控（全局单监控器，引用计数启停） ----

// 帧间隔分桶上界（ms）；60fps 正常帧 ≈16.7，>33 即肉眼可感掉帧
const BUCKETS = [17, 25, 33, 50, 100, Infinity]
let watchRefs = 0
let watchPeak = 0
let watchRafId: number | null = null
let watchStart = 0
let lastFrame = 0
let frameCounts: number[] = []
let worstFrame = 0
let mdAtStart: MdCounters = mdSnap()

function frameLoop(now: number) {
  if (lastFrame > 0) {
    const dt = now - lastFrame
    // >1s 视为窗口后台 rAF 暂停，不计入
    if (dt < 1000) {
      worstFrame = Math.max(worstFrame, dt)
      frameCounts[BUCKETS.findIndex(b => dt < b)]++
    }
  }
  lastFrame = now
  watchRafId = requestAnimationFrame(frameLoop)
}

export function frameWatchRetain() {
  if (!enabled) return
  watchRefs++
  watchPeak = Math.max(watchPeak, watchRefs)
  if (watchRafId !== null) return
  watchStart = performance.now()
  lastFrame = 0
  frameCounts = BUCKETS.map(() => 0)
  worstFrame = 0
  watchPeak = watchRefs
  mdAtStart = mdSnap()
  watchRafId = requestAnimationFrame(frameLoop)
}

export function frameWatchRelease() {
  if (!enabled || watchRefs === 0) return
  watchRefs--
  if (watchRefs > 0 || watchRafId === null) return
  cancelAnimationFrame(watchRafId)
  watchRafId = null
  const total = frameCounts.reduce((a, b) => a + b, 0)
  if (total === 0) return
  const dropped33 = frameCounts.slice(3).reduce((a, b) => a + b, 0)
  const dropped50 = frameCounts.slice(4).reduce((a, b) => a + b, 0)
  const d = mdDiff(mdAtStart)
  const dur = (performance.now() - watchStart) / 1000
  console.log(
    `${TAG} 流式帧监控 · 时长 ${dur.toFixed(1)}s · ${total} 帧 · 并发峰值 ${watchPeak} 路\n` +
      `  帧间隔分布: <17ms ${frameCounts[0]} · 17-25 ${frameCounts[1]} · 25-33 ${frameCounts[2]} · 33-50 ${frameCounts[3]} · 50-100 ${frameCounts[4]} · >100 ${frameCounts[5]}\n` +
      `  掉帧: >33ms ${dropped33} 帧（${((dropped33 / total) * 100).toFixed(1)}%）· >50ms ${dropped50} 帧 · 最差 ${fmt(worstFrame)}\n` +
      `  流式期 plain parse: 累计 ${fmt(d.plainMs)} / ${d.plainN} 次 · 均摊 ${d.plainN ? fmt(d.plainMs / d.plainN) : '0ms'}/次`,
    TAG_STYLE,
  )
}

// ---- 3. 流式结束翻转帧（streaming prop 翻 false → shiki 一次性上色） ----

export function probeFinishFlip(sessionId: string) {
  if (!enabled) return
  const t0 = performance.now()
  const mdBefore = mdSnap()
  void (async () => {
    await nextFrame() // Vue 微任务 patch（含 shiki 同步渲染）之后的首个 rAF
    await nextFrame() // 首帧 paint 完成
    const dt = performance.now() - t0
    const d = mdDiff(mdBefore)
    console.log(
      `${TAG} 流式结束翻转 ${sessionId.slice(0, 8)}: ${fmt(dt)}（shiki render ${fmt(d.renderMs)} · LRU ${d.hit} hit / ${d.miss} miss）`,
      TAG_STYLE,
    )
  })()
}
