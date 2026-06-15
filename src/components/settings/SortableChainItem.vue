<script setup lang="ts">
import { ref } from 'vue'
import { useSortable } from '@dnd-kit/vue/sortable'

const props = defineProps<{ id: string; index: number; group: string }>()
const el = ref<HTMLElement>()

const { isDragging } = useSortable({
  id: `${props.group}:${props.id}`,
  index: () => props.index,
  element: el,
  group: props.group,
})
</script>

<template>
  <div
    ref="el"
    class="chain-item"
    :class="{ 'chain-item-dragging': isDragging }"
  >
    <span class="i-carbon-draggable w-3 h-3 text-muted-foreground/50 shrink-0" />
    <slot />
  </div>
</template>

<style scoped>
.chain-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--card);
  cursor: grab;
  touch-none: manipulation;
  transition: box-shadow 0.15s, opacity 0.15s;
}
.chain-item:hover {
  box-shadow: var(--shadow-paper);
}
.chain-item:active {
  cursor: grabbing;
}
.chain-item-dragging {
  opacity: 0.4;
  box-shadow: none;
}
</style>
