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
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-notebook w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">NotebookEdit</span>
      <span v-if="notebookPath" class="font-mono text-default3 truncate" :title="notebookPath">{{ notebookPath }}</span>
      <span v-if="cellId" class="px-1.5 py-0.5 rounded bg-default4/15 text-default3 font-mono">cell {{ cellId }}</span>
      <span v-if="cellType" class="px-1.5 py-0.5 rounded bg-blue-500/15 text-blue-400">{{ cellType }}</span>
      <span v-if="editMode" class="px-1.5 py-0.5 rounded bg-orange-500/15 text-orange-400">{{ editMode }}</span>
    </div>
    <div v-if="oldSource || newSource" class="mt-2 space-y-1">
      <pre v-if="oldSource" class="rounded bg-red-500/10 border border-red-500/20 px-2 py-1 text-red-300 whitespace-pre-wrap break-all font-mono">- {{ oldSource }}</pre>
      <pre v-if="newSource" class="rounded bg-green-500/10 border border-green-500/20 px-2 py-1 text-green-300 whitespace-pre-wrap break-all font-mono">+ {{ newSource }}</pre>
    </div>
  </div>
</template>
