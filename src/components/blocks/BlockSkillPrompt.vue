<script setup lang="ts">
import { ref } from 'vue'

defineProps<{
  block: { type: 'skill_prompt'; text: string; name: string; [key: string]: unknown }
}>()

const expanded = ref(false)
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs text-primary hover:text-primary/80 flex items-center gap-1"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-play-filled-alt w-3 h-3" />
      调用技能：{{ block.name }}
    </button>
    <div
      v-if="expanded"
      class="mt-1 pl-3 border-l-2 border-border text-xs text-muted-foreground whitespace-pre-wrap max-h-64 overflow-y-auto"
    >
      {{ block.text }}
    </div>
  </div>
</template>
