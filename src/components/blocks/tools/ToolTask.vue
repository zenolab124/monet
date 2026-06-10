<script setup lang="ts">
import { computed, ref } from 'vue'

const PROMPT_PREVIEW_LEN = 120

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const subagentType = computed(() => {
  const v = props.input.subagent_type
  return typeof v === 'string' ? v : ''
})

const description = computed(() => {
  const v = props.input.description
  return typeof v === 'string' ? v : ''
})

const prompt = computed(() => {
  const v = props.input.prompt
  return typeof v === 'string' ? v : ''
})

const expanded = ref(false)

const isLong = computed(() => prompt.value.length > PROMPT_PREVIEW_LEN)

const promptPreview = computed(() => {
  if (!isLong.value || expanded.value) return prompt.value
  return prompt.value.slice(0, PROMPT_PREVIEW_LEN) + '…'
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-task w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">{{ name }}</span>
      <span v-if="subagentType" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground font-mono">{{ subagentType }}</span>
      <span v-if="description" class="text-foreground">{{ description }}</span>
    </div>
    <div v-if="prompt" class="mt-1 text-muted-foreground whitespace-pre-wrap break-words">{{ promptPreview }}</div>
    <button
      v-if="isLong"
      class="mt-1 text-muted-foreground hover:text-foreground"
      @click="expanded = !expanded"
    >
      {{ expanded ? '收起' : '展开' }}
    </button>
  </div>
</template>
