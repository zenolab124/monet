<script setup lang="ts">
import { computed } from 'vue'

const PROMPT_PREVIEW_LEN = 80

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const url = computed(() => {
  const v = props.input.url
  return typeof v === 'string' ? v : ''
})

const prompt = computed(() => {
  const v = props.input.prompt
  return typeof v === 'string' ? v : ''
})

const promptPreview = computed(() => {
  if (prompt.value.length <= PROMPT_PREVIEW_LEN) return prompt.value
  return prompt.value.slice(0, PROMPT_PREVIEW_LEN) + '…'
})
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-cloud w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">WebFetch</span>
      <a
        v-if="url"
        :href="url"
        target="_blank"
        rel="noopener noreferrer"
        class="font-mono text-blue-400 hover:text-blue-300 truncate underline-offset-2 hover:underline"
        :title="url"
      >{{ url }}</a>
    </div>
    <div v-if="promptPreview" class="mt-1 text-default3">{{ promptPreview }}</div>
  </div>
</template>
