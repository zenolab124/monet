<script setup lang="ts">
import { computed } from 'vue'
import { parseQuestions } from '@/utils/askQuestions'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const questions = computed(() => parseQuestions(props.input))

/** 已答会话的回显:input.answers 经权限组件注入后落盘,key=问题文本 */
const answers = computed<Record<string, string | string[]>>(() => {
  const raw = props.input.answers
  return typeof raw === 'object' && raw !== null && !Array.isArray(raw)
    ? (raw as Record<string, string | string[]>)
    : {}
})

function isChosen(question: string, label: string): boolean {
  const a = answers.value[question]
  return Array.isArray(a) ? a.includes(label) : a === label
}

/** answers 值不匹配任何选项 label 时,是「其他」自由文本 */
function customAnswer(question: string, options: { label: string }[]): string {
  const a = answers.value[question]
  const vals = Array.isArray(a) ? a : typeof a === 'string' ? [a] : []
  return vals.filter(v => v && !options.some(o => o.label === v)).join('、')
}
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-help w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">询问用户</span>
    </div>
    <div v-for="(q, qi) in questions" :key="qi" class="mt-2">
      <div class="flex items-center gap-1.5 flex-wrap">
        <span v-if="q.header" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground">{{ q.header }}</span>
        <span class="text-foreground font-medium">{{ q.question }}</span>
        <span v-if="q.multiSelect" class="text-muted-foreground">（可多选）</span>
      </div>
      <ul class="mt-1.5 space-y-1">
        <li v-for="(opt, oi) in q.options" :key="oi" class="flex gap-1.5 items-baseline">
          <span
            class="w-3 h-3 shrink-0 translate-y-0.5"
            :class="isChosen(q.question, opt.label)
              ? 'i-carbon-radio-button-checked text-primary'
              : 'i-carbon-radio-button text-muted-foreground'"
          />
          <span :class="isChosen(q.question, opt.label) ? 'text-foreground font-medium' : 'text-foreground'">{{ opt.label }}</span>
          <span v-if="opt.description" class="text-muted-foreground">— {{ opt.description }}</span>
        </li>
        <li v-if="customAnswer(q.question, q.options)" class="flex gap-1.5 items-baseline">
          <span class="i-carbon-radio-button-checked w-3 h-3 shrink-0 translate-y-0.5 text-primary" />
          <span class="text-foreground font-medium">{{ customAnswer(q.question, q.options) }}</span>
          <span class="text-muted-foreground">— 自定义回答</span>
        </li>
      </ul>
    </div>
  </div>
</template>
