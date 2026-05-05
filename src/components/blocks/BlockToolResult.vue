<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'tool_result' }>
}>()

const expanded = ref(false)

/** 把嵌套 content 拍平成纯文本（递归处理 ContentBlock[] 中的 text 块） */
function flattenText(content: string | ContentBlock[]): string {
  if (typeof content === 'string') return content
  return content
    .filter((b): b is Extract<ContentBlock, { type: 'text' }> => b.type === 'text')
    .map(b => b.text)
    .join('\n')
}

const fullText = computed(() => flattenText(props.block.content))
const preview = computed(() => fullText.value.slice(0, 80))
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs flex items-center gap-1"
      :class="block.is_error ? 'text-red-400' : 'text-default4 hover:text-default3'"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
      → 结果
      <span v-if="block.is_error" class="text-red-400">（错误）</span>
      <span v-if="!expanded" class="text-default4 font-normal truncate max-w-48">
        {{ preview }}{{ fullText.length > 80 ? '…' : '' }}
      </span>
    </button>
    <div
      v-if="expanded"
      class="mt-1 pl-3 border-l-2 text-xs whitespace-pre-wrap"
      :class="block.is_error ? 'border-red-500/30 text-red-300' : 'border-default4/30 text-default3'"
    >
      {{ fullText }}
    </div>
  </div>
</template>
