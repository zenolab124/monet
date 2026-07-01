<script setup lang="ts">
import { computed, ref, inject } from 'vue'
import type { SubAgentMeta } from '@/types'

const PROMPT_PREVIEW_LEN = 120

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const findSubAgent = inject<(toolUseId: string) => SubAgentMeta | undefined>('findSubAgent')
const toggleSubAgent = inject<(meta: SubAgentMeta) => void>('toggleSubAgent')
const isSubAgentOpen = inject<(agentId: string) => boolean>('isSubAgentOpen')

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

const matchedAgent = computed(() => findSubAgent?.(props.toolUseId))
const isOpen = computed(() =>
  matchedAgent.value ? isSubAgentOpen?.(matchedAgent.value.agent_id) ?? false : false,
)

function onToggle() {
  const meta = matchedAgent.value
  if (meta && toggleSubAgent) toggleSubAgent(meta)
}
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-bot w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">{{ name }}</span>
      <span v-if="subagentType" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground font-mono">{{ subagentType }}</span>
      <span v-if="description" class="text-foreground">{{ description }}</span>
      <button
        v-if="matchedAgent"
        class="ml-auto px-1.5 py-0.5 rounded border transition-colors"
        :class="isOpen
          ? 'border-primary bg-primary/15 text-primary'
          : 'border-primary/30 text-primary hover:bg-primary/10'"
        @click="onToggle"
      >
        {{ isOpen ? $t('block.toolTask.collapse') : $t('block.toolTask.viewAgent') }}
      </button>
    </div>
    <div v-if="prompt" class="mt-1 text-muted-foreground whitespace-pre-wrap break-words">{{ promptPreview }}</div>
    <button
      v-if="isLong"
      class="mt-1 text-muted-foreground hover:text-foreground"
      @click="expanded = !expanded"
    >
      {{ expanded ? $t('block.toolTask.collapse') : $t('block.toolTask.expand') }}
    </button>
  </div>
</template>
