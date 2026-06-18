<script setup lang="ts">
defineProps<{
  block: { type: 'system-event'; text: string; [key: string]: unknown }
}>()
</script>

<template>
  <div class="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground/70">
    <span class="i-carbon-flash w-3 h-3 shrink-0" />
    <span>{{ block.text }}</span>
  </div>
</template>
