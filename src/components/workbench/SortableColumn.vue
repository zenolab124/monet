<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSortable } from '@dnd-kit/vue/sortable'

const props = defineProps<{ tabId: string; index: number; flex: number }>()
const el = ref<HTMLElement>()

const { isDragging } = useSortable({
  id: computed(() => `col:${props.tabId}:${props.index}`),
  index: () => props.index,
  group: 'columns',
  element: el,
})
</script>

<template>
  <div
    ref="el"
    class="min-w-0 h-full relative wb-col touch-none"
    :style="{ flex: `${flex} 1 0%` }"
  >
    <slot :is-dragging="isDragging" />
  </div>
</template>
