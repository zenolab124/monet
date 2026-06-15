<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ContentBlock } from '@/types'

const { t } = useI18n()

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'document' }>
}>()

/** media_type → 用户可读的文档类型名 */
const docKind = computed(() => {
  const mt = props.block.source.media_type
  if (mt.includes('pdf')) return t('block.pdfDoc')
  if (mt.includes('text')) return t('block.textDoc')
  return t('block.docGeneric', { mime: mt })
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2">
    <div class="text-xs text-muted-foreground flex items-center gap-1.5">
      <span class="i-carbon-document w-3.5 h-3.5" />
      {{ docKind }}<template v-if="block.title">{{ $t('block.docTitle', { title: block.title }) }}</template>
    </div>
  </div>
</template>
