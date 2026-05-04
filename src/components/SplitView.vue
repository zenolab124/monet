<script setup lang="ts">
import { ref } from 'vue'
import { useSplitLayout } from '@/composables/useSplitLayout'
import SessionDetail from './SessionDetail.vue'

const { state, updateSize, setActivePane } = useSplitLayout()

const containerRef = ref<HTMLElement>()
const dragging = ref(false)

/**
 * 拖动第 index 条分隔线：
 *   prefixLeft = sum(sizes[0..index]) * rect.width  （panes[index] 起点像素）
 *   leftPx     = e.clientX - rect.left - prefixLeft
 *   leftRatio  = leftPx / rect.width                （panes[index] 占整体的新比例）
 */
function onMouseDown(e: MouseEvent, index: number) {
  e.preventDefault()
  dragging.value = true
  const rect = containerRef.value?.getBoundingClientRect()
  if (!rect) return

  const prefix = state.value.sizes
    .slice(0, index)
    .reduce((s, v) => s + v, 0)
  const prefixLeft = prefix * rect.width

  const onMouseMove = (ev: MouseEvent) => {
    const leftPx = ev.clientX - rect.left - prefixLeft
    const leftRatio = leftPx / rect.width
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
  <div ref="containerRef" class="h-full flex flex-row">
    <template v-for="(pane, i) in state.panes" :key="pane.id">
      <!-- 面板本体 -->
      <div
        class="min-w-0 h-full overflow-hidden split-child"
        :class="{ 'no-transition': dragging }"
        :style="{ width: `${state.sizes[i] * 100}%` }"
        @mousedown="setActivePane(pane.id)"
      >
        <SessionDetail :pane-id="pane.id" />
      </div>

      <!-- 分隔线：视觉 1px，交互区域 8px。最后一个 pane 之后不渲染 -->
      <div
        v-if="i < state.panes.length - 1"
        class="shrink-0 relative w-0 border-l border-divider"
      >
        <div
          class="absolute top-0 bottom-0 -left-4px w-9px cursor-col-resize z-10"
          @mousedown="onMouseDown($event, i)"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.split-child {
  transition: width 200ms cubic-bezier(0.32, 0.72, 0, 1);
}
.split-child.no-transition {
  transition: none;
}
</style>
