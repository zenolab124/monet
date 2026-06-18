<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'

const expanded = ref(false)
const overflows = ref(false)
const contentEl = ref<HTMLElement>()

let ro: ResizeObserver | null = null

function checkOverflow() {
  const el = contentEl.value
  if (!el || expanded.value) return
  overflows.value = el.scrollHeight > el.clientHeight + 2
}

onMounted(() => {
  ro = new ResizeObserver(checkOverflow)
  if (contentEl.value) ro.observe(contentEl.value)
})

onUnmounted(() => {
  ro?.disconnect()
  ro = null
})
</script>

<template>
  <div ref="contentEl" class="overflow-hidden" :class="expanded ? '' : 'max-h-20'">
    <slot />
  </div>
  <div
    v-if="overflows"
    class="text-xs text-muted-foreground/70 mt-0.5 cursor-pointer select-none hover:text-muted-foreground transition-colors"
    @click="expanded = !expanded"
  >
    {{ expanded ? $t('common.collapseUp') : $t('common.expandDown') }}
  </div>
</template>
