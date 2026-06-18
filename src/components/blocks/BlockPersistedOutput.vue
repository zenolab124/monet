<script setup lang="ts">
import { ref, computed } from 'vue'

const props = defineProps<{
  block: { type: 'persisted-output' | 'user-prompt-submit-hook'; text: string; [key: string]: unknown }
}>()

const expanded = ref(false)

const meta = computed(() => {
  const text = props.block.text
  const sizeMatch = text.match(/Output too large \(([^)]+)\)/)
  const size = sizeMatch ? sizeMatch[1] : null
  const previewMatch = text.match(/Preview \(first [^)]+\):\n([\s\S]*)/)
  const preview = previewMatch ? previewMatch[1].trim() : text.trim()
  return { size, preview }
})
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-script w-3 h-3" />
      <span>{{ $t('block.hookOutput') }}</span>
      <span v-if="meta.size" class="opacity-60">({{ meta.size }})</span>
    </button>
    <pre v-if="expanded" class="mt-1 pl-3 border-l-2 border-default4/20 text-xs text-muted-foreground whitespace-pre-wrap break-all max-h-48 overflow-y-auto">{{ meta.preview }}</pre>
  </div>
</template>
