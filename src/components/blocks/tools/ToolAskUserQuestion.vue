<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

interface ParsedOption {
  label: string
  description: string
}

interface ParsedQuestion {
  header: string
  question: string
  multiSelect: boolean
  options: ParsedOption[]
}

/** input.questions 容错解析：任何字段缺失都降级为空，不抛错 */
const questions = computed<ParsedQuestion[]>(() => {
  const raw = props.input.questions
  if (!Array.isArray(raw)) return []
  return raw.map((q): ParsedQuestion => {
    const obj = (typeof q === 'object' && q !== null ? q : {}) as Record<string, unknown>
    const options = Array.isArray(obj.options)
      ? obj.options.map((o): ParsedOption => {
          const oo = (typeof o === 'object' && o !== null ? o : {}) as Record<string, unknown>
          return {
            label: typeof oo.label === 'string' ? oo.label : '',
            description: typeof oo.description === 'string' ? oo.description : '',
          }
        })
      : []
    return {
      header: typeof obj.header === 'string' ? obj.header : '',
      question: typeof obj.question === 'string' ? obj.question : '',
      multiSelect: obj.multiSelect === true,
      options,
    }
  })
})
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-help w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">询问用户</span>
    </div>
    <div v-for="(q, qi) in questions" :key="qi" class="mt-2">
      <div class="flex items-center gap-1.5 flex-wrap">
        <span v-if="q.header" class="px-1.5 py-0.5 rounded bg-cyan-500/15 text-cyan-400">{{ q.header }}</span>
        <span class="text-default2 font-medium">{{ q.question }}</span>
        <span v-if="q.multiSelect" class="text-default4">（可多选）</span>
      </div>
      <ul class="mt-1.5 space-y-1">
        <li v-for="(opt, oi) in q.options" :key="oi" class="flex gap-1.5 items-baseline">
          <span class="i-carbon-radio-button w-3 h-3 shrink-0 translate-y-0.5 text-default4" />
          <span class="text-default2">{{ opt.label }}</span>
          <span v-if="opt.description" class="text-default4">— {{ opt.description }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>
