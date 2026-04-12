<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSplitLayout, type SplitNode } from '@/composables/useSplitLayout'
import SessionDetail from './SessionDetail.vue'

const props = defineProps<{
  node: SplitNode
}>()

const { activePaneId, updateRatio, setActivePane } = useSplitLayout()

const containerRef = ref<HTMLElement>()
const dragging = ref(false)

const isActive = computed(() =>
  props.node.type === 'pane' && activePaneId.value === props.node.id,
)

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
    :class="{ 'ring-1 ring-blue-500/40 ring-inset': isActive }"
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

    <!-- 分割线 -->
    <div
      class="shrink-0 relative group"
      :class="node.axis === 'horizontal'
        ? 'w-1 cursor-col-resize hover:bg-blue-500/20'
        : 'h-1 cursor-row-resize hover:bg-blue-500/20'"
      @mousedown="onMouseDown"
    >
      <div
        class="absolute bg-blue-500/40 transition-opacity"
        :class="[
          dragging ? 'opacity-100' : 'opacity-0 group-hover:opacity-100',
          node.axis === 'horizontal'
            ? 'left-0 top-1/2 -translate-y-1/2 w-full h-8 rounded-full'
            : 'top-0 left-1/2 -translate-x-1/2 h-full w-8 rounded-full',
        ]"
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
