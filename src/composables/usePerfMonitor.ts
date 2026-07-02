import { ref, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** Rust get_perf_stats 返回结构（src-tauri/src/perf.rs） */
export interface ProcMem {
  pid: number
  name: string
  footprint_mb: number
}
export interface PerfStats {
  main: ProcMem
  webkit: ProcMem[]
  cli: ProcMem[]
  total_mb: number
}

// ---- 采样状态（模块单例；仅 HUD 打开期间运行） ----

const fps = ref(0)
/** 观察到的刷新率基线（ProMotion 120Hz 机器上自动上调，分级阈值随之校准） */
const baselineFps = ref(60)
/** 近 10s 帧间隔超过 2.5 个基线帧周期的次数（丢帧） */
const jankCount = ref(0)
/** 近 10s 最长帧间隔（主线程最长阻塞的近似下界） */
const maxBlockMs = ref(0)
/** 最近一次 pointerdown → 下一帧的延迟（点击响应的直接度量） */
const clickLatencyLast = ref(0)
/** 近 100 次点击延迟的 p95 */
const clickLatencyP95 = ref(0)
const domNodes = ref(0)
const memStats = ref<PerfStats | null>(null)
/** projects-changed 事件计数（验证增量更新是否生效） */
const projEvents = reactive({ incremental: 0, full: 0 })
/** 每秒一个 FPS 采样点，最多 60 个（sparkline 数据） */
const fpsHistory = ref<number[]>([])

let running = false
let rafId = 0
let lastFrameTs = 0
let framesThisSecond = 0
let secondStartTs = 0
// 10 个 1s 桶的滑动窗口
const jankBuckets: number[] = []
const blockBuckets: number[] = []
let bucketJank = 0
let bucketMaxBlock = 0
const clickSamples: number[] = []
let memTimer = 0
let domTimer = 0
let unlistenProj: UnlistenFn | null = null
// 代际计数：listen() 是异步的，start→stop→start 交错时旧 listen 的 resolve
// 会覆盖新监听造成泄漏，await 后校验代际未变才落地
let startGen = 0

function frameLoop(ts: number) {
  if (!running) return
  if (lastFrameTs > 0) {
    const interval = ts - lastFrameTs
    // 丢帧阈值随基线刷新率校准：60Hz→41.7ms、120Hz→20.8ms（均为连丢 2 帧）
    if (interval > 2500 / baselineFps.value) bucketJank++
    if (interval > bucketMaxBlock) bucketMaxBlock = interval
    framesThisSecond++
    if (ts - secondStartTs >= 1000) {
      fps.value = framesThisSecond
      if (framesThisSecond > baselineFps.value) baselineFps.value = framesThisSecond
      fpsHistory.value = [...fpsHistory.value.slice(-59), framesThisSecond]
      jankBuckets.push(bucketJank)
      blockBuckets.push(bucketMaxBlock)
      if (jankBuckets.length > 10) jankBuckets.shift()
      if (blockBuckets.length > 10) blockBuckets.shift()
      jankCount.value = jankBuckets.reduce((a, b) => a + b, 0)
      maxBlockMs.value = Math.round(Math.max(...blockBuckets))
      framesThisSecond = 0
      secondStartTs = ts
      bucketJank = 0
      bucketMaxBlock = 0
    }
  } else {
    secondStartTs = ts
  }
  lastFrameTs = ts
  rafId = requestAnimationFrame(frameLoop)
}

function onPointerDown(e: PointerEvent) {
  const t0 = e.timeStamp
  requestAnimationFrame((ts) => {
    const latency = Math.max(0, ts - t0)
    clickLatencyLast.value = Math.round(latency)
    clickSamples.push(latency)
    if (clickSamples.length > 100) clickSamples.shift()
    const sorted = [...clickSamples].sort((a, b) => a - b)
    clickLatencyP95.value = Math.round(sorted[Math.floor(sorted.length * 0.95)] ?? latency)
  })
}

async function pollMem() {
  try {
    memStats.value = await invoke<PerfStats>('get_perf_stats')
  } catch (_) {
    // 采集失败保持旧值
  }
}

function countDom() {
  domNodes.value = document.getElementsByTagName('*').length
}

/** 启动采样（HUD 挂载时调用）。重复调用幂等 */
export async function startPerfMonitor() {
  if (running) return
  running = true
  const gen = ++startGen
  lastFrameTs = 0
  rafId = requestAnimationFrame(frameLoop)
  window.addEventListener('pointerdown', onPointerDown, { capture: true, passive: true })
  pollMem()
  countDom()
  memTimer = window.setInterval(pollMem, 2000)
  domTimer = window.setInterval(countDom, 5000)
  const un = await listen<{ full: boolean }>('projects-changed', (event) => {
    if (event.payload?.full) projEvents.full++
    else projEvents.incremental++
  })
  // await 期间发生过 stop 或新一轮 start：本次监听作废，立即释放
  if (gen !== startGen || !running) {
    un()
    return
  }
  unlistenProj = un
}

/** 停止采样并释放所有监听（HUD 卸载时调用） */
export function stopPerfMonitor() {
  running = false
  startGen++
  cancelAnimationFrame(rafId)
  window.removeEventListener('pointerdown', onPointerDown, { capture: true } as EventListenerOptions)
  clearInterval(memTimer)
  clearInterval(domTimer)
  unlistenProj?.()
  unlistenProj = null
  // 清滑窗，避免重开 HUD 显示陈旧丢帧数据
  jankBuckets.length = 0
  blockBuckets.length = 0
  bucketJank = 0
  bucketMaxBlock = 0
  framesThisSecond = 0
}

export function usePerfMonitor() {
  return {
    fps,
    baselineFps,
    jankCount,
    maxBlockMs,
    clickLatencyLast,
    clickLatencyP95,
    domNodes,
    memStats,
    projEvents,
    fpsHistory,
  }
}
