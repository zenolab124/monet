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

const oldString = computed(() => {
  const v = props.input.old_string
  return typeof v === 'string' ? v : ''
})

const newString = computed(() => {
  const v = props.input.new_string
  return typeof v === 'string' ? v : ''
})

const replaceAll = computed(() => props.input.replace_all === true)
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-edit w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">Edit</span>
      <span v-if="filePath" class="font-mono text-default3 truncate" :title="filePath">{{ filePath }}</span>
      <span v-if="replaceAll" class="ml-1 px-1.5 py-0.5 rounded bg-orange-500/15 text-orange-400 text-2xs">全部替换</span>
    </div>
    <div v-if="oldString || newString" class="mt-2 space-y-1">
      <pre v-if="oldString" class="rounded bg-red-500/10 border border-red-500/20 px-2 py-1 text-red-300 whitespace-pre-wrap break-all font-mono">- {{ oldString }}</pre>
      <pre v-if="newString" class="rounded bg-green-500/10 border border-green-500/20 px-2 py-1 text-green-300 whitespace-pre-wrap break-all font-mono">+ {{ newString }}</pre>
    </div>
  </div>
</template>
