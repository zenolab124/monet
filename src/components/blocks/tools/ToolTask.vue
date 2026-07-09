<script setup lang="ts">
import { computed, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SubAgentMeta } from '@/types'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const { t } = useI18n()

const findSubAgent = inject<(toolUseId: string) => SubAgentMeta | undefined>('findSubAgent')
const toggleSubAgent = inject<(meta: SubAgentMeta) => void>('toggleSubAgent')
const isSubAgentOpen = inject<(agentId: string) => boolean>('isSubAgentOpen')

const isWorkflow = computed(() => props.name === 'Workflow')

const typeLabel = computed(() => {
  if (isWorkflow.value) return 'Workflow'
  const v = props.input.subagent_type
  return typeof v === 'string' && v ? v : props.name
})

const description = computed(() => {
  const v = props.input.description
  return typeof v === 'string' ? v : ''
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
  <div
    class="mt-2 flex items-center gap-1.5 px-2.5 py-1.5 rounded text-[11px]
           bg-background border border-border cursor-pointer
           transition-colors hover:bg-card hover:border-primary/40"
    :class="isOpen && 'border-primary bg-primary/5'"
    :data-tool-use-id="toolUseId"
    @click="onToggle"
  >
    <span
      class="w-4 h-4 shrink-0 rounded flex items-center justify-center text-[9px] font-semibold"
      :class="isWorkflow
        ? 'bg-claude/15 text-claude'
        : 'bg-tag text-tag-foreground'"
    >{{ isWorkflow ? 'W' : 'A' }}</span>
    <span class="font-medium text-foreground shrink-0">{{ typeLabel }}</span>
    <span v-if="description" class="text-muted-foreground truncate flex-1">{{ description }}</span>
    <span
      v-if="matchedAgent"
      class="shrink-0 text-[10px] text-primary tabular-nums"
    >{{ t('block.toolTask.viewDetail') }}</span>
  </div>
</template>
