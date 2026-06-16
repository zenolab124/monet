<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'image' }>
}>()

const expanded = ref(false)

const dataUrl = computed(() => {
  const { media_type, data } = props.block.source
  if (!data) return null
  return `data:${media_type};base64,${data}`
})

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') expanded.value = false
}
</script>

<template>
  <div v-if="dataUrl" class="mt-1">
    <img
      :src="dataUrl"
      :alt="$t('block.image', { mime: block.source.media_type, size: block.source.data.length })"
      class="block-image-thumb"
      @click="expanded = true"
    />
    <Teleport to="body">
      <div
        v-if="expanded"
        class="block-image-overlay"
        @click="expanded = false"
        @keydown="onKeydown"
        tabindex="0"
      >
        <img :src="dataUrl" class="block-image-full" />
      </div>
    </Teleport>
  </div>
  <div v-else class="mt-2 rounded-md bg-background border border-border px-3 py-2">
    <div class="text-xs text-muted-foreground flex items-center gap-1.5">
      <span class="i-carbon-image w-3.5 h-3.5" />
      {{ $t('block.image', { mime: block.source.media_type, size: 0 }) }}
    </div>
  </div>
</template>

<style scoped>
.block-image-thumb {
  max-width: 300px;
  max-height: 200px;
  border-radius: 6px;
  border: 1px solid var(--border);
  cursor: zoom-in;
  object-fit: contain;
  background: var(--muted);
}
.block-image-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(0 0 0 / 0.7);
  cursor: zoom-out;
  backdrop-filter: blur(4px);
}
.block-image-full {
  max-width: 90vw;
  max-height: 90vh;
  object-fit: contain;
  border-radius: 8px;
}
</style>
