<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'

const CLAMP_PX = 160
const FADE_MASK = 'linear-gradient(to bottom, #000 calc(100% - 28px), transparent)'

const expanded = ref(false)
const overflows = ref(false)
const wrapperEl = ref<HTMLElement>()
const contentEl = ref<HTMLElement>()

let ro: ResizeObserver | null = null

function check() {
  const el = contentEl.value
  if (!el) return
  overflows.value = el.scrollHeight > CLAMP_PX + 2
  if (!overflows.value) expanded.value = false
}

function expand() {
  if (expanded.value || !overflows.value || !wrapperEl.value || !contentEl.value) return
  const to = Math.min(contentEl.value.scrollHeight, window.innerHeight * 0.5)
  expanded.value = true
  wrapperEl.value.animate(
    { maxHeight: [`${CLAMP_PX}px`, `${to}px`] },
    { duration: 220, easing: 'cubic-bezier(0.4, 0, 0.2, 1)' },
  )
}

function collapse() {
  if (!expanded.value || !wrapperEl.value) return
  const from = wrapperEl.value.offsetHeight
  const anim = wrapperEl.value.animate(
    { maxHeight: [`${from}px`, `${CLAMP_PX}px`] },
    { duration: 180, easing: 'cubic-bezier(0.4, 0, 0.2, 1)', fill: 'forwards' },
  )
  anim.onfinish = () => {
    anim.cancel()
    expanded.value = false
  }
}

onMounted(() => {
  ro = new ResizeObserver(check)
  if (contentEl.value) ro.observe(contentEl.value)
})

onUnmounted(() => {
  ro?.disconnect()
})
</script>

<template>
  <div
    :class="!expanded && overflows ? 'cursor-pointer' : ''"
    @click="!expanded && overflows && expand()"
  >
    <div
      ref="wrapperEl"
      class="overflow-hidden"
      :style="{
        maxHeight: expanded ? '50vh' : `${CLAMP_PX}px`,
        overflowY: expanded ? 'auto' : 'hidden',
        WebkitMaskImage: !expanded && overflows ? FADE_MASK : undefined,
        maskImage: !expanded && overflows ? FADE_MASK : undefined,
      }"
    >
      <div ref="contentEl">
        <slot />
      </div>
    </div>
  </div>
  <div v-if="expanded && overflows" class="flex justify-center mt-1">
    <button
      class="inline-flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground/70 hover:text-muted-foreground rounded-full hover:bg-muted/50 transition-colors select-none"
      @click.stop="collapse"
    >
      <span class="i-lucide-chevron-up text-2.5" />
      {{ $t('common.collapseUp') }}
    </button>
  </div>
</template>
