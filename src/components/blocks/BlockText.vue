<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import type { ContentBlock } from '@/types'
import { renderMarkdownPlain, renderMarkdownCached, renderMarkdownDeferred } from '@/composables/useMarkdown'

const TEXT_TRUNCATE_LEN = 8192
const MIN_STABLE_LEN = 200

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'text' }>
  streaming?: boolean
}>()

const expanded = ref(false)
const isLargeText = computed(() => props.block.text.length > TEXT_TRUNCATE_LEN)
const displayText = computed(() => {
  if (expanded.value || !isLargeText.value) return props.block.text
  return props.block.text.slice(0, TEXT_TRUNCATE_LEN)
})

// ---- 段落分段:流式期只重 parse 活跃尾部,已完成段落缓存 HTML ----

function findSafeSplit(text: string): number {
  let inFence = false
  let last = -1
  for (let i = 0; i < text.length; i++) {
    if ((i === 0 || text[i - 1] === '\n') && text[i] === '`' && text[i + 1] === '`' && text[i + 2] === '`') {
      inFence = !inFence
      i += 2
      continue
    }
    if (!inFence && text[i] === '\n' && text[i + 1] === '\n') {
      last = i + 2
      i++
    }
  }
  return last
}

const stableHtml = ref('')
const stableLen = ref(0)

watch(() => props.streaming ? displayText.value : null, (text) => {
  if (!text) return
  const split = findSafeSplit(text)
  if (split > stableLen.value && split >= MIN_STABLE_LEN) {
    stableHtml.value = renderMarkdownPlain(text.slice(0, split))
    stableLen.value = split
  }
})

// ---- 渐进 shiki:流式结束不同帧 burst,排队逐块上色 ----

const deferredHtml = ref('')
const wasStreaming = ref(false)

watch(() => props.streaming, (now, was) => {
  if (was && !now) {
    console.log(`%c ========== [BlockText] streaming→false, trigger shiki len=${displayText.value.length} t=${performance.now().toFixed(0)} ==========`, 'color:#8b5cf6;font-weight:bold')
    wasStreaming.value = true
    renderMarkdownDeferred(displayText.value).then(html => {
      console.log(`%c ========== [BlockText] shiki done len=${html.length} t=${performance.now().toFixed(0)} ==========`, 'color:#8b5cf6;font-weight:bold')
      deferredHtml.value = html
      stableHtml.value = ''
      stableLen.value = 0
    })
  }
})

function onProseClick(e: MouseEvent) {
  const btn = (e.target as HTMLElement).closest('.code-copy-btn')
  if (!btn) return
  e.preventDefault()
  const pre = btn.closest('.code-block-wrapper')?.querySelector('pre')
  if (!pre) return
  navigator.clipboard.writeText(pre.textContent ?? '').then(() => {
    btn.setAttribute('data-copied', '')
    setTimeout(() => btn.removeAttribute('data-copied'), 1500)
  })
}

const renderedHtml = computed(() => {
  const pendingShiki = wasStreaming.value && !deferredHtml.value
  if (props.streaming || pendingShiki) {
    const text = displayText.value
    if (stableLen.value > 0) {
      const tail = text.slice(stableLen.value)
      return stableHtml.value + (tail ? renderMarkdownPlain(tail) : '')
    }
    return renderMarkdownPlain(text)
  }
  if (wasStreaming.value) {
    return deferredHtml.value
  }
  return renderMarkdownCached(displayText.value)
})
</script>

<template>
  <div class="prose-msg text-sm" @click="onProseClick">
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
