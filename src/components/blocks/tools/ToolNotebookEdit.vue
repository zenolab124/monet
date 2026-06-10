<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const notebookPath = computed(() => {
  const v = props.input.notebook_path
  return typeof v === 'string' ? v : ''
})

const cellId = computed(() => {
  const v = props.input.cell_id
  return typeof v === 'string' ? v : ''
})

const cellType = computed(() => {
  const v = props.input.cell_type
  return typeof v === 'string' ? v : ''
})

const editMode = computed(() => {
  const v = props.input.edit_mode
  return typeof v === 'string' ? v : ''
})

const newSource = computed(() => {
  const v = props.input.new_source
  return typeof v === 'string' ? v : ''
})

const oldSource = computed(() => {
  // NotebookEdit 通常没有 old_source,但保留兜底
  const v = props.input.old_source
  return typeof v === 'string' ? v : ''
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-notebook w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">NotebookEdit</span>
      <span v-if="notebookPath" class="font-mono text-muted-foreground truncate" :title="notebookPath">{{ notebookPath }}</span>
      <span v-if="cellId" class="px-1.5 py-0.5 rounded bg-muted text-muted-foreground font-mono">cell {{ cellId }}</span>
      <span v-if="cellType" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground">{{ cellType }}</span>
      <span v-if="editMode" class="px-1.5 py-0.5 rounded border border-accent/50 text-accent">{{ editMode }}</span>
    </div>
    <div v-if="oldSource || newSource" class="mt-2 space-y-1">
      <pre v-if="oldSource" class="rounded bg-destructive/10 border border-destructive/20 px-2 py-1 text-destructive whitespace-pre-wrap break-all font-mono">- {{ oldSource }}</pre>
      <pre v-if="newSource" class="rounded bg-primary/10 border border-primary/20 px-2 py-1 text-primary whitespace-pre-wrap break-all font-mono">+ {{ newSource }}</pre>
    </div>
  </div>
</template>
