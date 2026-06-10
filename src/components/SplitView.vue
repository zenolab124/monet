<script setup lang="ts">
import { ref } from 'vue'
import { useSplitLayout } from '@/composables/useSplitLayout'
import SessionDetail from './SessionDetail.vue'

const { state, updateSize, setActivePane } = useSplitLayout()

const containerRef = ref<HTMLElement>()
const dragging = ref(false)

/** 浮起布局（FR-008）：四周边距与 pane 间隙均为 10px，比例分配只作用于自由空间 */
const PAD = 10
const GAP = 10

/**
 * 拖动第 index 条分隔线（pane 按 flex-grow 比例分配自由空间）：
 *   freeWidth  = rect.width - 两侧边距 - 全部间隙
 *   prefixLeft = sum(sizes[0..index]) * freeWidth + PAD + GAP*index （panes[index] 起点像素）
 *   leftRatio  = (e.clientX - rect.left - prefixLeft) / freeWidth   （panes[index] 新比例）
 */
function onMouseDown(e: MouseEvent, index: number) {
  e.preventDefault()
  dragging.value = true
  const rect = containerRef.value?.getBoundingClientRect()
  if (!rect) return

  const freeWidth = rect.width - PAD * 2 - GAP * (state.value.panes.length - 1)
  const prefix = state.value.sizes
    .slice(0, index)
    .reduce((s, v) => s + v, 0)
  const prefixLeft = prefix * freeWidth + PAD + GAP * index

  const onMouseMove = (ev: MouseEvent) => {
    const leftPx = ev.clientX - rect.left - prefixLeft
    const leftRatio = leftPx / freeWidth
    updateSize(index, leftRatio)
  }

  const onMouseUp = () => {
    dragging.value = false
    document.removeEventListener('mousemove', onMouseMove)
    document.removeEventListener('mouseup', onMouseUp)
  }

  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
}
</script>

<template>
  <div ref="containerRef" class="h-full flex flex-row p-2.5">
    <template v-for="(pane, i) in state.panes" :key="pane.id">
      <!-- 面板本体：浮起的纸（card 底 + border + 圆角 + paper 阴影，内容圆角裁切） -->
      <div
        class="min-w-0 h-full overflow-hidden split-child
               bg-card border border-border rounded shadow-paper"
        :class="{ 'no-transition': dragging }"
        :style="{ flex: `${state.sizes[i]} 1 0%` }"
        @mousedown="setActivePane(pane.id)"
      >
        <SessionDetail :pane-id="pane.id" />
      </div>

      <!-- pane 间隙 10px：拖拽命中区 9px 居中其内。最后一个 pane 之后不渲染 -->
      <div
        v-if="i < state.panes.length - 1"
        class="shrink-0 relative w-2.5"
      >
        <div
          class="absolute top-0 bottom-0 left-1/2 -translate-x-1/2 w-9px cursor-col-resize z-10"
          @mousedown="onMouseDown($event, i)"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.split-child {
  transition: flex 200ms cubic-bezier(0.32, 0.72, 0, 1);
}
.split-child.no-transition {
  transition: none;
}
</style>
