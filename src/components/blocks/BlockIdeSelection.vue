<script setup lang="ts">
import { ref, computed } from 'vue'
import { fileName as baseName } from '@/utils/path'

const props = defineProps<{
  block: { type: 'ide_selection'; text: string; [key: string]: unknown }
}>()

const expanded = ref(false)

const meta = computed(() => {
  const m = props.block.text.match(/selected the lines? (\d+)(?: to (\d+))? from (.+?):?\n/)
  if (!m) return { file: '', lineStart: '', lineEnd: '', content: props.block.text }
  const file = m[3].trim()
  const fileName = baseName(file)
  const content = props.block.text.slice(m.index! + m[0].length).replace(/\n*This may or may not be related to the current task\.\s*$/, '').trim()
  return {
    fileName,
    file,
    lineStart: m[1],
    lineEnd: m[2] || m[1],
    content,
  }
})
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-text-selection w-3 h-3" />
      <span>{{ $t('block.ideSelection', { name: meta.fileName, start: meta.lineStart, end: meta.lineEnd }) }}</span>
    </button>
    <pre v-if="expanded" class="mt-1 pl-3 border-l-2 border-default4/20 text-xs text-muted-foreground whitespace-pre-wrap break-all max-h-48 overflow-y-auto">{{ meta.content }}</pre>
  </div>
</template>
