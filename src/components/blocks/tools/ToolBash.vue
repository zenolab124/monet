<script setup lang="ts">
import { computed, ref, inject, type ComputedRef } from 'vue'
import { flattenResultText, type ToolResultData } from '@/utils/toolPair'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const command = computed(() => {
  const v = props.input.command
  return typeof v === 'string' ? v : ''
})

const description = computed(() => {
  const v = props.input.description
  return typeof v === 'string' ? v : ''
})

const runInBackground = computed(() => props.input.run_in_background === true)

const toolResultMap = inject<ComputedRef<Map<string, ToolResultData>>>('toolResultMap')
const result = computed(() => toolResultMap?.value.get(props.toolUseId))

const resultText = computed(() => {
  if (!result.value) return ''
  return flattenResultText(result.value.content)
})

const expanded = ref(false)

const copiedIn = ref(false)
const copiedOut = ref(false)

async function copyText(text: string, target: 'in' | 'out') {
  await navigator.clipboard.writeText(text)
  const flag = target === 'in' ? copiedIn : copiedOut
  flag.value = true
  setTimeout(() => flag.value = false, 1500)
}
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 min-w-0">
      <span class="i-carbon-terminal w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium shrink-0">Bash</span>
      <span v-if="description" class="text-muted-foreground truncate" :title="description">{{ description }}</span>
      <span v-if="runInBackground" class="ml-auto px-1.5 py-0.5 rounded border border-border text-muted-foreground shrink-0">后台</span>
    </div>

    <div v-if="command" class="group mt-1.5 flex items-center gap-1.5">
      <span class="text-muted-foreground/60 shrink-0 font-mono text-2xs">输入</span>
      <code class="flex-1 truncate font-mono text-foreground" :title="command">{{ command }}</code>
      <button
        class="opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
        title="复制命令"
        @click="copyText(command, 'in')"
      >
        <span v-if="copiedIn" class="i-carbon-checkmark w-3 h-3 text-primary" />
        <span v-else class="i-carbon-copy w-3 h-3 text-muted-foreground hover:text-foreground" />
      </button>
    </div>

    <div v-if="resultText" class="group mt-1.5 pt-1.5 border-t border-border/40 flex items-start gap-1.5">
      <span
        class="shrink-0 font-mono text-2xs mt-px"
        :class="result?.is_error ? 'text-destructive/60' : 'text-muted-foreground/60'"
      >输出</span>
      <pre
        class="flex-1 min-w-0 font-mono whitespace-pre-wrap break-all cursor-pointer select-text out-clamp"
        :class="[
          result?.is_error ? 'text-destructive' : 'text-muted-foreground',
          expanded ? '' : 'line-clamp-3',
        ]"
        @click="expanded = !expanded"
      >{{ resultText }}</pre>
      <button
        class="opacity-0 group-hover:opacity-100 transition-opacity shrink-0 mt-px"
        title="复制输出"
        @click.stop="copyText(resultText, 'out')"
      >
        <span v-if="copiedOut" class="i-carbon-checkmark w-3 h-3 text-primary" />
        <span v-else class="i-carbon-copy w-3 h-3 text-muted-foreground hover:text-foreground" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.out-clamp.line-clamp-3 {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 3;
  overflow: hidden;
}
</style>
