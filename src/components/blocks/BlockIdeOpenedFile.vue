<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  block: { type: 'ide_opened_file'; text: string; [key: string]: unknown }
}>()

const filePath = computed(() => {
  const m = props.block.text.match(/file\s+(\/\S+)/)
  return m ? m[1] : ''
})

const fileName = computed(() => {
  const path = filePath.value
  if (path) return path.split('/').pop() || path
  return props.block.text
})
</script>

<template>
  <div class="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
    <span class="i-carbon-document w-3 h-3 shrink-0" />
    <span>{{ $t('block.openedFile', { name: fileName }) }}</span>
  </div>
</template>
