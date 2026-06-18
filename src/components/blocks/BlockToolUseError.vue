<script setup lang="ts">
defineProps<{
  block: { type: 'tool_use_error'; text: string; [key: string]: unknown }
}>()
</script>

<template>
  <div class="mt-1 flex items-start gap-1.5 text-xs text-destructive/80">
    <span class="i-carbon-warning-alt w-3 h-3 shrink-0 mt-0.5" />
    <span>{{ block.text }}</span>
  </div>
</template>
