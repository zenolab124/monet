<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const filePath = computed(() => {
  const v = props.input.file_path
  return typeof v === 'string' ? v : ''
})

const offset = computed(() => {
  const v = props.input.offset
  return typeof v === 'number' ? v : null
})

const limit = computed(() => {
  const v = props.input.limit
  return typeof v === 'number' ? v : null
})

const lineRange = computed(() => {
  if (offset.value === null && limit.value === null) return ''
  const start = offset.value ?? 1
  const end = limit.value !== null ? start + limit.value : null
  return end !== null ? `L${start}-${end}` : `L${start}-`
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-document w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Read</span>
      <span v-if="filePath" class="font-mono text-muted-foreground truncate" :title="filePath">{{ filePath }}</span>
      <span v-if="lineRange" class="font-mono text-muted-foreground ml-1">{{ lineRange }}</span>
    </div>
  </div>
</template>
