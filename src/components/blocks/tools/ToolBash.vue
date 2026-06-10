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
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-terminal w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Bash</span>
      <span v-if="runInBackground" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground">后台</span>
      <span v-if="timeout !== null" class="px-1.5 py-0.5 rounded bg-muted text-muted-foreground font-mono">timeout: {{ timeout }}ms</span>
    </div>
    <pre v-if="command" class="mt-2 rounded bg-muted px-2 py-1.5 text-foreground whitespace-pre-wrap break-all font-mono">$ {{ command }}</pre>
    <div v-if="description" class="mt-1 text-muted-foreground">{{ description }}</div>
  </div>
</template>
