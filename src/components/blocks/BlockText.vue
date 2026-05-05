<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'
import { renderMarkdown } from '@/composables/useMarkdown'

const TEXT_TRUNCATE_LEN = 8192

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'text' }>
}>()

const expanded = ref(false)

const isLargeText = computed(() => props.block.text.length > TEXT_TRUNCATE_LEN)

const displayText = computed(() => {
  if (expanded.value || !isLargeText.value) return props.block.text
  return props.block.text.slice(0, TEXT_TRUNCATE_LEN)
})

const renderedHtml = computed(() => renderMarkdown(displayText.value))
</script>

<template>
  <div class="prose-msg text-sm">
    <div v-html="renderedHtml" />
    <button
      v-if="isLargeText"
      class="text-xs text-primary hover:text-primary/80 ml-1"
      @click="expanded = !expanded"
    >
      {{ expanded ? '收起' : `…展开全部（${Math.round(block.text.length / 1024)}KB）` }}
    </button>
  </div>
</template>
