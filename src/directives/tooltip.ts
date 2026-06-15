import type { Directive } from 'vue'

let tipEl: HTMLDivElement | null = null
let hideTimer: ReturnType<typeof setTimeout> | null = null
let showTimer: ReturnType<typeof setTimeout> | null = null

function ensureTipEl(): HTMLDivElement {
  if (tipEl) return tipEl
  tipEl = document.createElement('div')
  tipEl.className = 'v-tooltip'
  tipEl.style.cssText = `
    position: fixed;
    z-index: 9999;
    max-width: 320px;
    padding: 6px 10px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--foreground);
    background: var(--popover);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-paper);
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s;
    word-wrap: break-word;
    white-space: pre-wrap;
  `
  document.body.appendChild(tipEl)
  return tipEl
}

function show(el: HTMLElement, text: string) {
  if (hideTimer) { clearTimeout(hideTimer); hideTimer = null }
  const tip = ensureTipEl()
  tip.textContent = text

  tip.style.opacity = '0'
  tip.style.display = 'block'

  const rect = el.getBoundingClientRect()
  const tipRect = tip.getBoundingClientRect()

  let top = rect.top - tipRect.height - 6
  let left = rect.left + (rect.width - tipRect.width) / 2

  if (top < 4) top = rect.bottom + 6
  if (left < 4) left = 4
  if (left + tipRect.width > window.innerWidth - 4) {
    left = window.innerWidth - tipRect.width - 4
  }

  tip.style.top = `${top}px`
  tip.style.left = `${left}px`
  tip.style.opacity = '1'
}

function hide() {
  if (showTimer) { clearTimeout(showTimer); showTimer = null }
  if (!tipEl) return
  tipEl.style.opacity = '0'
  hideTimer = setTimeout(() => {
    if (tipEl) tipEl.style.display = 'none'
  }, 150)
}

const SYMBOL = Symbol('tooltip')

interface TooltipBinding {
  onEnter: () => void
  onLeave: () => void
  text: string
}

export const vTooltip: Directive<HTMLElement, string> = {
  mounted(el, binding) {
    const b: TooltipBinding = {
      text: binding.value ?? '',
      onEnter: () => {
        if (!b.text) return
        showTimer = setTimeout(() => show(el, b.text), 400)
      },
      onLeave: hide,
    }
    ;(el as any)[SYMBOL] = b
    el.addEventListener('mouseenter', b.onEnter)
    el.addEventListener('mouseleave', b.onLeave)
  },
  updated(el, binding) {
    const b = (el as any)[SYMBOL] as TooltipBinding | undefined
    if (b) b.text = binding.value ?? ''
  },
  unmounted(el) {
    const b = (el as any)[SYMBOL] as TooltipBinding | undefined
    if (b) {
      el.removeEventListener('mouseenter', b.onEnter)
      el.removeEventListener('mouseleave', b.onLeave)
    }
    hide()
  },
}
