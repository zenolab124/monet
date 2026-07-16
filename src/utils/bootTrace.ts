/**
 * 启动白屏打点（生产常开，performance.mark 开销微秒级）。
 * 时间轴（均相对 timeOrigin = WebView 导航开始）：
 *   boot:html        index.html 内联脚本 —— 资产响应 + HTML 解析到位
 *   boot:js-start    模块图开始执行（本模块 body；须为 main.ts 首个 import 且零依赖）
 *                    与 boot:html 的差 = 主 chunk fetch + parse
 *   boot:modules     全部依赖模块 body 执行完（vue / markdown-it / shiki 发起等）
 *   boot:mounted     createApp().mount() 返回 —— 首帧 vdom→DOM（含六视图常驻挂载）
 *   boot:raf1        rAF —— 跑在下一渲染帧开头；mounted→raf1 = 主线程被 JS 长任务占住、渲染排不上队的等待
 *   boot:first-frame rAF 内 setTimeout(0) —— 首帧渲染完成后的首个宏任务；raf1→first-frame = 首帧真实 style/layout/paint
 *   boot:app-ready   App.vue onMounted 初始化链完成（已不白屏，量数据段用）
 */
performance.mark('boot:js-start')

export function markBoot(name: string) {
  performance.mark(name)
}

/**
 * mount 后调用：捕获首帧上屏时刻。
 * rAF 回调跑在下一渲染帧开头 → boot:raf1，量「主线程被 JS 占住、渲染排不上队」的等待；
 * 帧内 setTimeout(0) → boot:first-frame，宏任务紧随本帧 style/layout/paint 之后执行。
 * 不用 double-rAF（第二帧开头）：宏任务优先级高于 idle callback，第二帧则会被空闲期
 * 任务推迟——实测 Shiki 空闲预热 583ms 被整段计入白屏，量出伪影。
 */
export function finishBootTrace() {
  requestAnimationFrame(() => {
    performance.mark('boot:raf1')
    setTimeout(() => performance.mark('boot:first-frame'), 0)
  })
}

export interface BootSegment {
  /** i18n key 后缀：perf.boot_<label> */
  label: 'nav' | 'load' | 'exec' | 'render' | 'block' | 'paint' | 'data' | 'total' | 'shiki' | 'projects'
  ms: number
  /** true = 独立计时：与瀑布段时间重叠，不计入合计，HUD 单独区分展示。有对应 mark 才产出。 */
  overlay?: boolean
}

/** HUD 读取：按时间轴切段。关键 mark 缺失（dev HMR 重载等）返回空数组。 */
export function getBootBreakdown(): BootSegment[] {
  const at = (n: string) => performance.getEntriesByName(n)[0]?.startTime
  const html = at('boot:html')
  const js = at('boot:js-start')
  const mods = at('boot:modules')
  const mounted = at('boot:mounted')
  const raf1 = at('boot:raf1')
  const frame = at('boot:first-frame')
  const ready = at('boot:app-ready')
  if (html == null || js == null || mods == null || mounted == null || frame == null) return []
  const segs: BootSegment[] = [
    { label: 'nav', ms: html },
    { label: 'load', ms: js - html },
    { label: 'exec', ms: mods - js },
    { label: 'render', ms: mounted - mods },
  ]
  // mounted→first-frame 拆两段：block（主线程被 JS 占住、渲染排不上队的等待）+ paint（首帧真实绘制）。
  // raf1 缺失（旧 mark / dev 兼容）时退化为原来的单段 paint（mounted→first-frame）。
  if (raf1 != null) {
    segs.push({ label: 'block', ms: raf1 - mounted })
    segs.push({ label: 'paint', ms: frame - raf1 })
  } else {
    segs.push({ label: 'paint', ms: frame - mounted })
  }
  if (ready != null) segs.push({ label: 'data', ms: ready - frame })
  // 白屏总长 = 导航 → 首帧上屏
  segs.push({ label: 'total', ms: frame })

  // 独立计时（overlay）：与上面的瀑布段时间重叠，不参与加和，仅有对应 mark 时才产出。
  // mark 名是与并行任务的契约：Shiki 初始化打 boot:shiki-start/ready，项目树加载打 boot:projects-start/done。
  const shikiStart = at('boot:shiki-start')
  const shikiReady = at('boot:shiki-ready')
  if (shikiStart != null && shikiReady != null) {
    segs.push({ label: 'shiki', ms: shikiReady - shikiStart, overlay: true })
  }
  const projStart = at('boot:projects-start')
  const projDone = at('boot:projects-done')
  if (projStart != null && projDone != null) {
    segs.push({ label: 'projects', ms: projDone - projStart, overlay: true })
  }
  return segs
}
