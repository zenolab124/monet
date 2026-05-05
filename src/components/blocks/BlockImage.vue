<script setup lang="ts">
import type { ContentBlock } from '@/types'

defineProps<{
  block: Extract<ContentBlock, { type: 'image' }>
}>()
</script>

<template>
  <div class="mt-2 rounded-md bg-orange-500/5 border border-orange-500/20 px-3 py-2">
    <div class="text-xs text-orange-400 flex items-center gap-1.5">
      <span class="i-carbon-image w-3.5 h-3.5" />
      图片（{{ block.source.media_type }}，{{ block.source.data_length }} bytes）
    </div>
  </div>
</template>
