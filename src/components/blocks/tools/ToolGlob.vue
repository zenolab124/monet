<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const pattern = computed(() => {
  const v = props.input.pattern
  return typeof v === 'string' ? v : ''
})

const path = computed(() => {
  const v = props.input.path
  return typeof v === 'string' ? v : ''
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-folder-details w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Glob</span>
      <code v-if="pattern" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground font-mono">{{ pattern }}</code>
      <span v-if="path" class="font-mono text-muted-foreground truncate" :title="path">in {{ path }}</span>
    </div>
  </div>
</template>
