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

const glob = computed(() => {
  const v = props.input.glob
  return typeof v === 'string' ? v : ''
})

const path = computed(() => {
  const v = props.input.path
  return typeof v === 'string' ? v : ''
})

const outputMode = computed(() => {
  const v = props.input.output_mode
  return typeof v === 'string' ? v : ''
})

const type_ = computed(() => {
  const v = props.input.type
  return typeof v === 'string' ? v : ''
})
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-search w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">Grep</span>
      <code v-if="pattern" class="px-1.5 py-0.5 rounded bg-yellow-500/15 text-yellow-400 font-mono">{{ pattern }}</code>
      <span v-if="glob" class="font-mono text-default3">in {{ glob }}</span>
      <span v-if="path" class="font-mono text-default3 truncate" :title="path">{{ path }}</span>
      <span v-if="type_" class="px-1.5 py-0.5 rounded bg-default4/15 text-default3">type: {{ type_ }}</span>
      <span v-if="outputMode" class="px-1.5 py-0.5 rounded bg-default4/15 text-default3">{{ outputMode }}</span>
    </div>
  </div>
</template>
