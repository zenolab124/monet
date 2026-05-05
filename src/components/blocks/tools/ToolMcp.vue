<script setup lang="ts">
import { computed, ref } from 'vue'

const MCP_NAME_RE = /^mcp__([^_]+(?:_[^_]+)*?)__(.+)$/

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const parsed = computed(() => {
  const m = props.name.match(MCP_NAME_RE)
  if (!m) return null
  return { server: m[1], tool: m[2] }
})

const inputJson = computed(() => {
  try {
    return JSON.stringify(props.input, null, 2)
  } catch {
    return String(props.input)
  }
})

const expanded = ref(false)
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <button
      class="flex items-center gap-1.5 w-full text-left flex-wrap"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform shrink-0" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-plug w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">mcp</span>
      <template v-if="parsed">
        <span class="text-default4">·</span>
        <code class="px-1.5 py-0.5 rounded bg-default4/15 text-default3 font-mono">{{ parsed.server }}</code>
        <span class="text-default4">·</span>
        <code class="px-1.5 py-0.5 rounded bg-blue-500/15 text-blue-400 font-mono">{{ parsed.tool }}</code>
      </template>
      <code v-else class="px-1.5 py-0.5 rounded bg-default4/15 text-default3 font-mono">{{ name }}</code>
    </button>
    <pre v-if="expanded" class="mt-2 rounded bg-default4/10 px-2 py-1 text-default3 whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ inputJson }}</pre>
  </div>
</template>
