<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const skillName = computed(() => {
  const v = props.input.skill
  return typeof v === 'string' ? v : ''
})

const args = computed(() => {
  const v = props.input.args
  if (v === null || v === undefined || v === '') return ''
  if (typeof v === 'string') return v
  return JSON.stringify(v)
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-play-filled-alt w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Skill</span>
      <code v-if="skillName" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground font-mono">{{ skillName }}</code>
      <span v-if="args" class="font-mono text-muted-foreground truncate" :title="args">{{ args }}</span>
      <span class="ml-auto text-muted-foreground">{{ $t('block.toolSkill.loading') }}</span>
    </div>
  </div>
</template>
