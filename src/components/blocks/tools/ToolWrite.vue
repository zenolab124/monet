<script setup lang="ts">
import { computed, inject, ref } from 'vue'
import { fileName } from '@/utils/path'

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

const displayName = computed(() => fileName(filePath.value))

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

// 直达文件账本(仅主对话入账的调用显示;子代理转录不入账,自然隐藏)
const hasLedgerAnchor = inject<(id: string) => boolean>('hasLedgerAnchor', () => false)
const openFileLedger = inject<(id: string) => boolean>('openFileLedger', () => false)
const inLedger = computed(() => hasLedgerAnchor(props.toolUseId))
</script>

<template>
  <div :data-tool-use-id="toolUseId" class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div
      class="flex items-center gap-1.5 w-full text-left cursor-pointer"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform shrink-0" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-document-add w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">Write</span>
      <span v-if="filePath" class="font-mono text-muted-foreground truncate" :title="filePath">{{ displayName }}</span>
      <span v-if="content" class="text-muted-foreground ml-1">{{ $t('block.toolWrite.chars', { length: content.length }) }}</span>
      <button
        v-if="inLedger"
        class="ml-auto p-0.5 rounded text-muted-foreground/60 hover:text-claude hover:bg-muted shrink-0"
        :title="$t('fileLedger.viewInLedger')"
        @click.stop="openFileLedger(toolUseId)"
      >
        <span class="i-carbon-catalog w-3.5 h-3.5" />
      </button>
    </div>
    <div v-if="expanded" class="mt-2">
      <pre class="rounded bg-muted px-2 py-1 text-muted-foreground whitespace-pre-wrap break-all font-mono max-h-96 overflow-y-auto">{{ displayContent }}</pre>
      <div v-if="isLarge" class="mt-1 text-muted-foreground">
        {{ $t('block.toolWrite.truncated', { truncateKB: Math.round(TRUNCATE_LEN / 1024), totalKB: sizeKb }) }}
      </div>
    </div>
  </div>
</template>
