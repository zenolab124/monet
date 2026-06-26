<script setup lang="ts">
import { ref, computed, watch, onUnmounted, nextTick } from 'vue'

export interface AnchorItem {
  index: number
  text: string
}

const props = defineProps<{
  anchors: AnchorItem[]
  scrollContainer: HTMLElement | undefined
}>()

const activeIndex = ref(-1)
let observer: IntersectionObserver | null = null

function resolveEl(index: number): HTMLElement | null {
  return props.scrollContainer?.querySelector<HTMLElement>(`[data-anchor-index="${index}"]`) ?? null
}

function setupObserver() {
  observer?.disconnect()
  if (!props.scrollContainer || !props.anchors.length) return

  observer = new IntersectionObserver(
    (entries) => {
      let topMost: { index: number; top: number } | null = null
      for (const entry of entries) {
        if (!entry.isIntersecting) continue
        const idx = Number(entry.target.getAttribute('data-anchor-index'))
        if (isNaN(idx)) continue
        const top = entry.boundingClientRect.top
        if (!topMost || top < topMost.top) topMost = { index: idx, top }
      }
      if (topMost) activeIndex.value = topMost.index
    },
    { root: props.scrollContainer, threshold: 0, rootMargin: '0px 0px -70% 0px' },
  )

  for (const a of props.anchors) {
    const el = resolveEl(a.index)
    if (el) observer.observe(el)
  }
}

watch(() => [props.anchors, props.scrollContainer] as const, () => {
  nextTick(setupObserver)
}, { flush: 'post' })

onUnmounted(() => observer?.disconnect())

function scrollTo(anchor: AnchorItem) {
  const el = resolveEl(anchor.index)
  if (!el || !props.scrollContainer) return
  props.scrollContainer.scrollTo({
    top: el.offsetTop,
    behavior: 'smooth',
  })
}

const hoveredIndex = ref(-1)
const hoverPos = ref({ x: 0, y: 0 })

function onDotEnter(e: MouseEvent, index: number) {
  hoveredIndex.value = index
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  hoverPos.value = { x: rect.right + 6, y: rect.top + rect.height / 2 }
}

const showNav = computed(() => props.anchors.length > 1)
</script>

<template>
  <div v-if="showNav" class="anchor-rail">
    <div
      v-for="a in anchors"
      :key="a.index"
      class="anchor-dot-wrap"
      @mouseenter="onDotEnter($event, a.index)"
      @mouseleave="hoveredIndex = -1"
      @click="scrollTo(a)"
    >
      <div
        class="anchor-dot"
        :class="{ active: activeIndex === a.index }"
      />
    </div>
  </div>
  <Teleport to="body">
    <Transition name="anchor-tip">
      <div
        v-if="hoveredIndex >= 0"
        class="anchor-tooltip"
        :style="{ left: hoverPos.x + 'px', top: hoverPos.y + 'px' }"
      >
        {{ anchors.find(a => a.index === hoveredIndex)?.text }}
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.anchor-rail {
  position: absolute;
  left: 7px;
  top: 0;
  bottom: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 12px 0;
  width: 20px;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
  pointer-events: none;
}
.anchor-rail::-webkit-scrollbar { display: none; }

.anchor-dot-wrap {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  cursor: pointer;
  flex-shrink: 0;
  pointer-events: auto;
}

.anchor-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--muted-foreground);
  opacity: 0.15;
  transition: all 0.15s ease;
}
.anchor-dot-wrap:hover .anchor-dot {
  opacity: 1;
  transform: scale(1.8);
  background: var(--primary);
}
.anchor-dot.active {
  opacity: 1;
  background: var(--primary);
  box-shadow: 0 0 6px color-mix(in srgb, var(--primary) 40%, transparent);
  width: 6px;
  height: 6px;
}
</style>

<style>
.anchor-tooltip {
  position: fixed;
  transform: translateY(-50%);
  background: var(--popover);
  border: 1px solid var(--border);
  color: var(--popover-foreground);
  font-size: 12px;
  line-height: 1.5;
  padding: 6px 10px;
  border-radius: 6px;
  white-space: nowrap;
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  z-index: 9999;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  pointer-events: none;
}

.anchor-tip-enter-active { transition: opacity 0.12s ease, transform 0.12s ease; }
.anchor-tip-leave-active { transition: opacity 0.08s ease; }
.anchor-tip-enter-from { opacity: 0; transform: translateY(-50%) translateX(-4px); }
.anchor-tip-leave-to { opacity: 0; transform: translateY(-50%); }
</style>
