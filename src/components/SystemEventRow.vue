<script setup lang="ts">
import { computed } from 'vue'
import type { SessionRecord } from '@/types'

const props = defineProps<{
  record: Extract<SessionRecord, { type: 'system' }>
}>()

/** api_error 的可读文案：优先 formatted，回落 message */
const errorText = computed(() => {
  const err = props.record.error
  if (!err) return props.record.content ?? 'API 错误'
  const formatted = err.formatted
  if (typeof formatted === 'string' && formatted) return formatted
  const message = err.message
  if (typeof message === 'string' && message) return message
  return 'API 错误'
})

/** compact 前的 token 数（compactMetadata.preTokens） */
const preTokens = computed(() => {
  const v = props.record.compactMetadata?.preTokens
  return typeof v === 'number' ? v : null
})
</script>

<template>
  <!-- compact 边界：分隔线形态 -->
  <div v-if="record.subtype === 'compact_boundary'" class="flex items-center gap-3 my-3 text-xs text-default4">
    <div class="flex-1 h-px bg-default/40" />
    <span class="flex items-center gap-1.5 shrink-0">
      <span class="i-carbon-compress w-3.5 h-3.5" />
      上下文已压缩<template v-if="preTokens">（压缩前 {{ Math.round(preTokens / 1000) }}k tokens）</template>
    </span>
    <div class="flex-1 h-px bg-default/40" />
  </div>

  <!-- API 错误：红色紧凑提示行（连续重试已在 messages 层折叠为末条） -->
  <div v-else-if="record.subtype === 'api_error'" class="my-2 rounded-md bg-red-500/5 border border-red-500/20 px-3 py-1.5 text-xs flex items-center gap-1.5 flex-wrap">
    <span class="i-carbon-warning-alt w-3.5 h-3.5 shrink-0 text-red-400" />
    <span class="text-red-400 font-medium">API 错误</span>
    <span class="text-default3 break-all">{{ errorText }}</span>
    <span v-if="record.retryAttempt" class="px-1.5 py-0.5 rounded bg-red-500/10 text-red-400/80 shrink-0">
      已重试 {{ record.retryAttempt }}{{ record.maxRetries ? ` / ${record.maxRetries}` : '' }} 次
    </span>
  </div>

  <!-- 其余 subtype：灰色一行兜底 -->
  <div v-else class="my-2 text-xs text-default4 flex items-center gap-1.5">
    <span class="i-carbon-information w-3.5 h-3.5 shrink-0" />
    <span class="font-mono">{{ record.subtype ?? 'system' }}</span>
    <span v-if="record.content" class="text-default3">{{ record.content }}</span>
  </div>
</template>
