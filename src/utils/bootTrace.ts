/**
 * 启动白屏打点（生产常开，performance.mark 开销微秒级）。
 * 时间轴（均相对 timeOrigin = WebView 导航开始）：
 *   boot:html        index.html 内联脚本 —— 资产响应 + HTML 解析到位
 *   boot:js-start    模块图开始执行（本模块 body；须为 main.ts 首个 import 且零依赖）
 *                    与 boot:html 的差 = 主 chunk fetch + parse
 *   boot:modules     全部依赖模块 body 执行完（vue / markdown-it / shiki 发起等）
 *   boot:mounted     createApp().mount() 返回 —— 首帧 vdom→DOM（含六视图常驻挂载）
 *   boot:first-frame double-rAF —— 首帧真实上屏（style/layout/paint 之后）
 *   boot:app-ready   App.vue onMounted 初始化链完成（已不白屏，量数据段用）
 */
performance.mark('boot:js-start')

export function markBoot(name: string) {
  performance.mark(name)
}

/** mount 后调用：double-rAF 捕获首帧上屏时刻 */
export function finishBootTrace() {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => performance.mark('boot:first-frame'))
  })
}

export interface BootSegment {
  /** i18n key 后缀：perf.boot_<label> */
  label: 'nav' | 'load' | 'exec' | 'render' | 'paint' | 'data' | 'total'
  ms: number
}

/** HUD 读取：按时间轴切段。关键 mark 缺失（dev HMR 重载等）返回空数组。 */
export function getBootBreakdown(): BootSegment[] {
  const at = (n: string) => performance.getEntriesByName(n)[0]?.startTime
  const html = at('boot:html')
  const js = at('boot:js-start')
  const mods = at('boot:modules')
  const mounted = at('boot:mounted')
  const frame = at('boot:first-frame')
  const ready = at('boot:app-ready')
  if (html == null || js == null || mods == null || mounted == null || frame == null) return []
  const segs: BootSegment[] = [
    { label: 'nav', ms: html },
    { label: 'load', ms: js - html },
    { label: 'exec', ms: mods - js },
    { label: 'render', ms: mounted - mods },
    { label: 'paint', ms: frame - mounted },
  ]
  if (ready != null) segs.push({ label: 'data', ms: ready - frame })
  // 白屏总长 = 导航 → 首帧上屏
  segs.push({ label: 'total', ms: frame })
  return segs
}
