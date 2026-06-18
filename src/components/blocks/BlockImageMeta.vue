<script setup lang="ts">
defineProps<{
  block: { type: 'image-meta'; text: string; [key: string]: unknown }
}>()
</script>

<template>
  <div class="mt-0.5 text-xs text-muted-foreground/50 flex items-center gap-1">
    <span class="i-carbon-image w-3 h-3 shrink-0" />
    <span>{{ block.text }}</span>
  </div>
</template>
