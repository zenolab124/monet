<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'

const TEXT_TRUNCATE_LEN = 8192

const props = defineProps<{
  block: ContentBlock
}>()

const thinkingExpanded = ref(false)
const toolResultExpanded = ref(false)
const textExpanded = ref(false)

const b = computed(() => props.block as Record<string, any>)

// 大文本截断
const isLargeText = computed(() =>
  props.block.type === 'text' && b.value.text.length > TEXT_TRUNCATE_LEN,
)
const displayText = computed(() => {
  if (props.block.type !== 'text') return ''
  if (textExpanded.value || !isLargeText.value) return b.value.text
  return b.value.text.slice(0, TEXT_TRUNCATE_LEN)
})

const toolInputDisplay = computed(() => {
  if (props.block.type !== 'tool_use') return ''
  const input = b.value.input
  if (b.value.name === 'Bash' && input && typeof input === 'object' && 'command' in input) {
    return String(input.command)
  }
  return JSON.stringify(input, null, 2)
})

function toolResultText(content: string | ContentBlock[]): string {
  if (typeof content === 'string') return content
  return content
    .filter(b => b.type === 'text')
    .map(b => (b as any).text)
    .join('\n')
}
</script>

<template>
  <!-- 文本块 -->
  <div v-if="block.type === 'text'" class="whitespace-pre-wrap break-words text-sm">
    {{ displayText }}
    <button
      v-if="isLargeText"
      class="text-xs text-primary hover:text-primary/80 ml-1"
      @click="textExpanded = !textExpanded"
    >
      {{ textExpanded ? '收起' : `…展开全部（${Math.round(b.text.length / 1024)}KB）` }}
    </button>
  </div>

  <!-- 思考块 -->
  <div v-else-if="block.type === 'thinking'" class="mt-2">
    <button
      class="text-xs text-default4 hover:text-default3 flex items-center gap-1"
      @click="thinkingExpanded = !thinkingExpanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': thinkingExpanded }" />
      思考过程（{{ b.thinking.length }} 字）
    </button>
    <div v-if="thinkingExpanded" class="mt-1 pl-3 border-l-2 border-default4/30 text-xs text-default3 whitespace-pre-wrap max-h-64 overflow-y-auto">
      {{ b.thinking }}
    </div>
  </div>

  <!-- 工具调用 -->
  <div v-else-if="block.type === 'tool_use'" class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2">
    <div class="text-xs font-medium text-green-400 flex items-center gap-1.5">
      <span class="i-carbon-terminal w-3.5 h-3.5" />
      {{ b.name }}
    </div>
    <pre class="mt-1 text-xs text-default3 whitespace-pre-wrap break-all max-h-32 overflow-y-auto">{{ toolInputDisplay }}</pre>
  </div>

  <!-- 工具结果 -->
  <div v-else-if="block.type === 'tool_result'" class="mt-1">
    <button
      class="text-xs flex items-center gap-1"
      :class="b.is_error ? 'text-red-400' : 'text-default4 hover:text-default3'"
      @click="toolResultExpanded = !toolResultExpanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': toolResultExpanded }" />
      → 结果
      <span v-if="b.is_error" class="text-red-400">（错误）</span>
    </button>
    <div
      v-if="toolResultExpanded"
      class="mt-1 pl-3 border-l-2 text-xs whitespace-pre-wrap max-h-48 overflow-y-auto"
      :class="b.is_error ? 'border-red-500/30 text-red-300' : 'border-default4/30 text-default3'"
    >
      {{ toolResultText(b.content) }}
    </div>
  </div>

  <!-- 图片块 -->
  <div v-else-if="block.type === 'image'" class="mt-2 rounded-md bg-orange-500/5 border border-orange-500/20 px-3 py-2">
    <div class="text-xs text-orange-400 flex items-center gap-1.5">
      <span class="i-carbon-image w-3.5 h-3.5" />
      图片（{{ b.source.media_type }}，{{ b.source.data_length }} bytes）
    </div>
  </div>
</template>
