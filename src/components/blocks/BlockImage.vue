<script setup lang="ts">
import type { ContentBlock } from '@/types'

defineProps<{
  block: Extract<ContentBlock, { type: 'image' }>
}>()
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2">
    <div class="text-xs text-muted-foreground flex items-center gap-1.5">
      <span class="i-carbon-image w-3.5 h-3.5" />
      {{ $t('block.image', { mime: block.source.media_type, size: block.source.data_length }) }}
    </div>
  </div>
</template>
