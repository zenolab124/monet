<script setup lang="ts">
import { computed } from 'vue'
import { fileName as baseName } from '@/utils/path'

const props = defineProps<{
  block: { type: 'ide_opened_file'; text: string; [key: string]: unknown }
}>()

const filePath = computed(() => {
  // 兼容 Unix 绝对路径与 Windows 盘符路径两种形态
  const m = props.block.text.match(/file\s+(\/\S+|[A-Za-z]:[/\\]\S+)/)
  return m ? m[1] : ''
})

const fileName = computed(() => {
  const path = filePath.value
  if (path) return baseName(path)
  return props.block.text
})
</script>

<template>
  <div class="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
    <span class="i-carbon-document w-3 h-3 shrink-0" />
    <span>{{ $t('block.openedFile', { name: fileName }) }}</span>
  </div>
</template>
