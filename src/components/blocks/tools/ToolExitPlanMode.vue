<script setup lang="ts">
import { computed } from 'vue'
import { renderMarkdown } from '@/composables/useMarkdown'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const plan = computed(() => {
  const v = props.input.plan
  return typeof v === 'string' ? v : ''
})

const renderedPlan = computed(() => renderMarkdown(plan.value))
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-checkmark-outline w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">ExitPlanMode</span>
      <span class="px-1.5 py-0.5 rounded border border-accent/50 text-accent">等待用户确认</span>
    </div>
    <div v-if="plan" class="mt-2 prose-msg" v-html="renderedPlan" />
  </div>
</template>
