<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'
import { renderMarkdown } from '@/composables/useMarkdown'

const TEXT_TRUNCATE_LEN = 8192

const props = defineProps<{
  block: ContentBlock
}>()

const thinkingExpanded = ref(false)
const toolInputExpanded = ref(false)
const toolResultExpanded = ref(false)
const textExpanded = ref(false)
const sysReminderExpanded = ref(false)
const skillPromptExpanded = ref(false)

/** 从 ide_opened_file 文本中提取文件路径 */
const ideFileName = computed(() => {
  if (props.block.type !== 'ide_opened_file') return ''
  const m = b.value.text?.match(/file\s+(\/\S+)/)
  return m ? m[1].split('/').pop() || m[1] : b.value.text
})

const ideFilePath = computed(() => {
  if (props.block.type !== 'ide_opened_file') return ''
  const m = b.value.text?.match(/file\s+(\/\S+)/)
  return m ? m[1] : ''
})

const TOOL_PREVIEW_LEN = 120

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

// markdown 渲染结果
const renderedHtml = computed(() => {
  if (props.block.type !== 'text') return ''
  return renderMarkdown(displayText.value)
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
  <div v-if="block.type === 'text'" class="prose-msg text-sm">
    <div v-html="renderedHtml" />
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
    <div v-if="thinkingExpanded" class="mt-1 pl-3 border-l-2 border-default4/30 text-xs text-default3 whitespace-pre-wrap">
      {{ b.thinking }}
    </div>
  </div>

  <!-- 工具调用 -->
  <div v-else-if="block.type === 'tool_use'" class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2">
    <button
      class="text-xs font-medium text-green-400 flex items-center gap-1.5 w-full text-left"
      @click="toolInputExpanded = !toolInputExpanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform shrink-0" :class="{ 'rotate-90': toolInputExpanded }" />
      <span class="i-carbon-terminal w-3.5 h-3.5 shrink-0" />
      <span>{{ b.name }}</span>
      <span v-if="!toolInputExpanded && toolInputDisplay.length > TOOL_PREVIEW_LEN" class="text-default4 font-normal truncate ml-1">
        {{ toolInputDisplay.slice(0, TOOL_PREVIEW_LEN) }}…
      </span>
      <span v-else-if="!toolInputExpanded" class="text-default4 font-normal truncate ml-1">
        {{ toolInputDisplay }}
      </span>
    </button>
    <pre v-if="toolInputExpanded" class="mt-1 text-xs text-default3 whitespace-pre-wrap break-all">{{ toolInputDisplay }}</pre>
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
      <span v-if="!toolResultExpanded" class="text-default4 font-normal truncate max-w-48">
        {{ toolResultText(b.content).slice(0, 80) }}{{ toolResultText(b.content).length > 80 ? '…' : '' }}
      </span>
    </button>
    <div
      v-if="toolResultExpanded"
      class="mt-1 pl-3 border-l-2 text-xs whitespace-pre-wrap"
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

  <!-- Skill 调用 -->
  <div v-else-if="block.type === 'skill_prompt'" class="mt-1">
    <button
      class="text-xs text-blue-400 hover:text-blue-300 flex items-center gap-1"
      @click="skillPromptExpanded = !skillPromptExpanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': skillPromptExpanded }" />
      <span class="i-carbon-play-filled-alt w-3 h-3" />
      调用技能：{{ b.name }}
    </button>
    <div v-if="skillPromptExpanded" class="mt-1 pl-3 border-l-2 border-blue-500/20 text-xs text-default4 whitespace-pre-wrap max-h-64 overflow-y-auto">
      {{ b.text }}
    </div>
  </div>

  <!-- IDE 文件打开 -->
  <div v-else-if="block.type === 'ide_opened_file'" class="mt-1 flex items-center gap-1.5 text-xs text-default4">
    <span class="i-carbon-document w-3 h-3 shrink-0" />
    <span>打开了 <span class="text-default3" :title="ideFilePath">{{ ideFileName }}</span></span>
  </div>

  <!-- 系统提醒 -->
  <div v-else-if="block.type === 'system-reminder'" class="mt-1">
    <button
      class="text-xs text-default4 hover:text-default3 flex items-center gap-1"
      @click="sysReminderExpanded = !sysReminderExpanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': sysReminderExpanded }" />
      <span class="i-carbon-information w-3 h-3" />
      系统提醒
    </button>
    <div v-if="sysReminderExpanded" class="mt-1 pl-3 border-l-2 border-default4/20 text-xs text-default4 whitespace-pre-wrap">
      {{ b.text }}
    </div>
  </div>

  <!-- 任务通知 -->
  <div v-else-if="block.type === 'task-notification'" class="mt-1 flex items-center gap-1.5 text-xs text-default4">
    <span class="i-carbon-task w-3 h-3 shrink-0" />
    <span>{{ b.text }}</span>
  </div>
</template>
