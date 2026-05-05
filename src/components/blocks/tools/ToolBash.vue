<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const command = computed(() => {
  const v = props.input.command
  return typeof v === 'string' ? v : ''
})

const description = computed(() => {
  const v = props.input.description
  return typeof v === 'string' ? v : ''
})

const runInBackground = computed(() => props.input.run_in_background === true)

const timeout = computed(() => {
  const v = props.input.timeout
  return typeof v === 'number' ? v : null
})
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-terminal w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">Bash</span>
      <span v-if="runInBackground" class="px-1.5 py-0.5 rounded bg-blue-500/15 text-blue-400">后台</span>
      <span v-if="timeout !== null" class="px-1.5 py-0.5 rounded bg-default4/15 text-default3 font-mono">timeout: {{ timeout }}ms</span>
    </div>
    <pre v-if="command" class="mt-2 rounded bg-default4/10 px-2 py-1.5 text-default2 whitespace-pre-wrap break-all font-mono">$ {{ command }}</pre>
    <div v-if="description" class="mt-1 text-default4">{{ description }}</div>
  </div>
</template>
