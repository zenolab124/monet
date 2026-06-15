<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'
import { renderMarkdownPlain, renderMarkdownCached } from '@/composables/useMarkdown'

const TEXT_TRUNCATE_LEN = 8192

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'text' }>
  /** 会话仍在流式中:走无高亮降级渲染;结束翻 false 后同实例切完整渲染,代码块一次性上色 */
  streaming?: boolean
}>()

const expanded = ref(false)

const isLargeText = computed(() => props.block.text.length > TEXT_TRUNCATE_LEN)

const displayText = computed(() => {
  if (expanded.value || !isLargeText.value) return props.block.text
  return props.block.text.slice(0, TEXT_TRUNCATE_LEN)
})

const renderedHtml = computed(() =>
  props.streaming ? renderMarkdownPlain(displayText.value) : renderMarkdownCached(displayText.value),
)
</script>

<template>
  <div class="prose-msg text-sm">
    <div v-html="renderedHtml" />
    <button
      v-if="isLargeText"
      class="text-xs text-primary hover:text-primary/80 ml-1"
      @click="expanded = !expanded"
    >
      {{ expanded ? $t('common.collapse') : $t('common.expandAll', { size: Math.round(block.text.length / 1024) }) }}
    </button>
  </div>
</template>
