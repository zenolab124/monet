<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'

const props = defineProps<{
  block: ContentBlock
}>()

const expanded = ref(false)

const json = computed(() => {
  try {
    return JSON.stringify(props.block, null, 2)
  } catch {
    return String(props.block)
  }
})
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-help w-3 h-3" />
      {{ $t('block.unknownType', { type: block.type }) }}
    </button>
    <pre v-if="expanded" class="mt-1 pl-3 border-l-2 border-default4/20 text-xs text-muted-foreground whitespace-pre-wrap break-all max-h-64 overflow-y-auto">{{ json }}</pre>
  </div>
</template>
