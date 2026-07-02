<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSortable } from '@dnd-kit/vue/sortable'

const props = defineProps<{ tabId: string; index: number; flex: number }>()
const el = ref<HTMLElement>()
const handleEl = ref<HTMLElement>()

const { isDragging } = useSortable({
  id: computed(() => `col:${props.tabId}:${props.index}`),
  index: () => props.index,
  group: 'columns',
  element: el,
  handle: handleEl,
})

const setHandle = (node: any) => { handleEl.value = node }
</script>

<template>
  <div
    ref="el"
    class="sortable-col"
    :class="{ 'sortable-col-dragging': isDragging }"
    :style="{ width: `${flex}px`, flex: '0 0 auto' }"
  >
    <slot :is-dragging="isDragging" :handle-ref="setHandle" />
  </div>
</template>

<style scoped>
.sortable-col {
  min-width: 0;
  height: 100%;
  position: relative;
  /* 列级 content-visibility 已移除（横滚顿挫治理）：它使新列进入 proximity 时
     整列可见带的全部消息组在同一渲染更新里原子解冻（50-200ms layout 尖峰，
     插值平滑吸收不了）。消息组级 cv（SessionDetail .msg-group-cv）的视口判定
     是二维的——屏外列的组照样被 skip，内存收益保留；而组进入 proximity 的
     时机各异，解冻天然分帧摊薄，横滚不再有整列尖峰 */
}
.sortable-col-dragging {
  opacity: 0.4;
}
</style>
