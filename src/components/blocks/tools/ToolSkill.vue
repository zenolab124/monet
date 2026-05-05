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
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-play-filled-alt w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">Skill</span>
      <code v-if="skillName" class="px-1.5 py-0.5 rounded bg-blue-500/15 text-blue-400 font-mono">{{ skillName }}</code>
      <span v-if="args" class="font-mono text-default3 truncate" :title="args">{{ args }}</span>
      <span class="ml-auto text-default4">加载中…</span>
    </div>
  </div>
</template>
