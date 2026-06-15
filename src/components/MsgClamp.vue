<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'

const expanded = ref(false)
const overflows = ref(false)
const contentEl = ref<HTMLElement>()

onMounted(() => {
  nextTick(() => {
    const el = contentEl.value
    if (el && el.scrollHeight > el.clientHeight + 2) {
      overflows.value = true
    }
  })
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
