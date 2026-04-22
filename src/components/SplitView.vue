<script setup lang="ts">
import { ref } from 'vue'
import { useSplitLayout, type SplitNode } from '@/composables/useSplitLayout'
import SessionDetail from './SessionDetail.vue'

const props = defineProps<{
  node: SplitNode
}>()

const { updateRatio, setActivePane } = useSplitLayout()

const containerRef = ref<HTMLElement>()
const dragging = ref(false)

function onMouseDown(e: MouseEvent) {
  if (props.node.type !== 'split') return
  e.preventDefault()
  dragging.value = true

  const splitId = props.node.id
  const axis = props.node.axis
  const rect = containerRef.value?.getBoundingClientRect()
  if (!rect) return

  const onMouseMove = (e: MouseEvent) => {
    if (!rect) return
    let ratio: number
    if (axis === 'horizontal') {
      ratio = (e.clientX - rect.left) / rect.width
    } else {
      ratio = (e.clientY - rect.top) / rect.height
    }
    updateRatio(splitId, ratio)
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
  <!-- 面板叶节点 -->
  <div
    v-if="node.type === 'pane'"
    class="h-full relative"
    @mousedown="setActivePane(node.id)"
  >
    <SessionDetail :pane-id="node.id" />
  </div>

  <!-- 分屏容器 -->
  <div
    v-else
    ref="containerRef"
    class="h-full flex"
    :class="node.axis === 'vertical' ? 'flex-col' : 'flex-row'"
  >
    <!-- 第一个子节点 -->
    <div
      :style="{
        [node.axis === 'horizontal' ? 'width' : 'height']: `${node.ratio * 100}%`,
      }"
      class="min-w-0 min-h-0 overflow-hidden"
    >
      <SplitView :node="node.first" />
    </div>

    <!-- 分割线：视觉 1px，交互区域 8px -->
    <div
      class="shrink-0 relative"
      :class="node.axis === 'horizontal'
        ? 'w-0 border-l border-divider'
        : 'h-0 border-t border-divider'"
    >
      <div
        class="absolute z-10"
        :class="node.axis === 'horizontal'
          ? 'top-0 bottom-0 -left-4px w-9px cursor-col-resize'
          : 'left-0 right-0 -top-4px h-9px cursor-row-resize'"
        @mousedown="onMouseDown"
      />
    </div>

    <!-- 第二个子节点 -->
    <div
      :style="{
        [node.axis === 'horizontal' ? 'width' : 'height']: `${(1 - node.ratio) * 100}%`,
      }"
      class="min-w-0 min-h-0 overflow-hidden"
    >
      <SplitView :node="node.second" />
    </div>
  </div>
</template>
