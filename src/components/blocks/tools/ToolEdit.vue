<script setup lang="ts">
import { computed, ref } from 'vue'
import { fileName } from '@/utils/path'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const filePath = computed(() => {
  const v = props.input.file_path
  return typeof v === 'string' ? v : ''
})

const displayName = computed(() => fileName(filePath.value))

const oldString = computed(() => {
  const v = props.input.old_string
  return typeof v === 'string' ? v : ''
})

const newString = computed(() => {
  const v = props.input.new_string
  return typeof v === 'string' ? v : ''
})

const replaceAll = computed(() => props.input.replace_all === true)

const expanded = ref(false)
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <button
      class="flex items-center gap-1.5 w-full text-left"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform shrink-0" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-edit w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Edit</span>
      <span v-if="filePath" class="font-mono text-muted-foreground truncate" :title="filePath">{{ displayName }}</span>
      <span v-if="replaceAll" class="ml-1 px-1.5 py-0.5 rounded border border-accent/50 text-accent text-2xs shrink-0">{{ $t('block.toolEdit.replaceAll') }}</span>
    </button>
    <div v-if="expanded && (oldString || newString)" class="mt-2 grid grid-cols-2 gap-1.5">
      <pre v-if="oldString" class="rounded bg-destructive/10 border border-destructive/20 px-2 py-1 text-destructive whitespace-pre-wrap break-all font-mono overflow-auto max-h-48">- {{ oldString }}</pre>
      <pre v-if="newString" class="rounded bg-primary/10 border border-primary/20 px-2 py-1 text-primary whitespace-pre-wrap break-all font-mono overflow-auto max-h-48">+ {{ newString }}</pre>
    </div>
  </div>
</template>
