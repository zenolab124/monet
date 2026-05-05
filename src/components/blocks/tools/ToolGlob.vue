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
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-folder-details w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">Glob</span>
      <code v-if="pattern" class="px-1.5 py-0.5 rounded bg-yellow-500/15 text-yellow-400 font-mono">{{ pattern }}</code>
      <span v-if="path" class="font-mono text-default3 truncate" :title="path">in {{ path }}</span>
    </div>
  </div>
</template>
