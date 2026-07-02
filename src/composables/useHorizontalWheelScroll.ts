import { onMounted, onUnmounted, type Ref } from 'vue'

/**
 * 工作台横向滚动接管（嵌套滚动 wheel 捕获，动机见 pitfalls/nested-scroll-wheel-capture.md）。
 *
 * 性能约束（docs/research/perf-audit-2026-07.md · P0-1）：
 * - deltaX 判断前置：纵向滚动事件（绝大多数）不读任何 layout 属性即返回，
 *   WebKit 手势级快速路径得以保留
 * - scrollLeft 写入走 rAF 合帧：120Hz 触控板事件合并到帧率，每帧至多一次布局读写
 * - 放行内层可横滚元素（代码块等横向溢出区），判定按 target 缓存：
 *   指针不动的连续滚动零 layout 读；元素被 v-html 重建替换时因身份变化自动重算。
 *   已知权衡：同一元素的溢出状态中途改变（流式追加）时判定短暂陈旧，指针移动后自愈
 */
export function useHorizontalWheelScroll(containerRef: Ref<HTMLElement | undefined>) {
  let pendingDelta = 0
  let rafId = 0
  let lastTarget: EventTarget | null = null
  let lastAllowInner = false

  function flush() {
    rafId = 0
    const el = containerRef.value
    const delta = pendingDelta
    pendingDelta = 0
    if (!el || delta === 0) return
    if (el.scrollWidth > el.clientWidth) el.scrollLeft += delta
  }

  /** target 到容器之间是否存在可横向滚动的祖先（如 pre 代码块） */
  function hasInnerHScroll(start: Element | null, boundary: HTMLElement): boolean {
    for (let node = start; node && node !== boundary; node = node.parentElement) {
      if (node.scrollWidth > node.clientWidth + 1) {
        const overflowX = getComputedStyle(node).overflowX
        if (overflowX === 'auto' || overflowX === 'scroll') return true
      }
    }
    return false
  }

  function onWheelCapture(e: WheelEvent) {
    if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return
    const el = containerRef.value
    if (!el) return
    if (e.target !== lastTarget) {
      lastTarget = e.target
      lastAllowInner = hasInnerHScroll(e.target as Element, el)
    }
    if (lastAllowInner) return
    e.preventDefault()
    pendingDelta += e.deltaX
    if (!rafId) rafId = requestAnimationFrame(flush)
  }

  onMounted(() => {
    containerRef.value?.addEventListener('wheel', onWheelCapture, { capture: true, passive: false })
  })

  onUnmounted(() => {
    containerRef.value?.removeEventListener('wheel', onWheelCapture, { capture: true } as EventListenerOptions)
    if (rafId) cancelAnimationFrame(rafId)
    lastTarget = null
  })
}
