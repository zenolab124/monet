<script setup lang="ts">
import { ref } from 'vue'
import { useSortable } from '@dnd-kit/vue/sortable'

const props = defineProps<{ tabId: string; index: number }>()
const el = ref<HTMLElement>()

const { isDragging, isDropTarget } = useSortable({
  id: 'tab:' + props.tabId,
  index: () => props.index,
  group: 'tabs',
  element: el,
})
</script>

<template>
  <div
    ref="el"
    class="sortable-tab"
    :class="{ 'sortable-tab-dragging': isDragging, 'sortable-tab-target': isDropTarget }"
  >
    <slot :is-dragging="isDragging" :is-drop-target="isDropTarget" />
  </div>
</template>

<style scoped>
.sortable-tab {
  display: inline-flex;
  flex-shrink: 0;
  cursor: grab;
  touch-action: none;
}
.sortable-tab:active {
  cursor: grabbing;
}
.sortable-tab-dragging {
  opacity: 0.3;
}
.sortable-tab-target {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
  border-radius: var(--radius);
}
</style>
