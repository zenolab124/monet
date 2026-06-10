<script setup lang="ts">
import { ref } from 'vue'

defineProps<{
  block: { type: 'system-reminder'; text: string; [key: string]: unknown }
}>()

const expanded = ref(false)
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-information w-3 h-3" />
      系统提醒
    </button>
    <div v-if="expanded" class="mt-1 pl-3 border-l-2 border-default4/20 text-xs text-muted-foreground whitespace-pre-wrap">
      {{ block.text }}
    </div>
  </div>
</template>
