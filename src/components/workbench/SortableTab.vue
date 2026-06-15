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
  accept: ['tabs', 'session-cards'],
})
</script>

<template>
  <div ref="el" :class="{ 'opacity-40': isDragging, 'ring-1 ring-primary': isDropTarget }" style="display: contents">
    <slot :is-dragging="isDragging" :is-drop-target="isDropTarget" />
  </div>
</template>
