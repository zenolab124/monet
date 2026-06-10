<script setup lang="ts">
import { computed, ref } from 'vue'

const TRUNCATE_LEN = 8192

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const filePath = computed(() => {
  const v = props.input.file_path
  return typeof v === 'string' ? v : ''
})

const content = computed(() => {
  const v = props.input.content
  return typeof v === 'string' ? v : ''
})

const expanded = ref(false)

const isLarge = computed(() => content.value.length > TRUNCATE_LEN)

const displayContent = computed(() => {
  if (expanded.value || !isLarge.value) return content.value
  return content.value.slice(0, TRUNCATE_LEN)
})

const sizeKb = computed(() => Math.round(content.value.length / 1024))
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <button
      class="flex items-center gap-1.5 w-full text-left"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform shrink-0" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-document-add w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Write</span>
      <span v-if="filePath" class="font-mono text-muted-foreground truncate" :title="filePath">{{ filePath }}</span>
      <span v-if="content" class="text-muted-foreground ml-1">（{{ content.length }} 字符）</span>
    </button>
    <div v-if="expanded" class="mt-2">
      <pre class="rounded bg-muted px-2 py-1 text-muted-foreground whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ displayContent }}</pre>
      <div v-if="isLarge" class="mt-1 text-muted-foreground">
        已截断显示前 {{ Math.round(TRUNCATE_LEN / 1024) }}KB（共 {{ sizeKb }}KB）
      </div>
    </div>
  </div>
</template>
