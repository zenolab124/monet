<script setup lang="ts">
import { ref, computed } from 'vue'

const props = defineProps<{
  block: { type: 'task-notification'; text: string; [key: string]: unknown }
}>()

const expanded = ref(false)

function extractTag(name: string): string {
  const m = props.block.text.match(new RegExp(`<${name}>([\\s\\S]*?)</${name}>`))
  return m ? m[1].trim() : ''
}

const status = computed(() => extractTag('status'))
const summary = computed(() => extractTag('summary'))
const result = computed(() => extractTag('result'))
const duration = computed(() => {
  const ms = Number(extractTag('duration_ms'))
  if (!ms) return ''
  return ms >= 60000 ? `${(ms / 60000).toFixed(1)}m` : ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`
})
const tokens = computed(() => {
  const t = Number(extractTag('total_tokens'))
  return t ? (t >= 1000 ? `${(t / 1000).toFixed(1)}k` : String(t)) : ''
})

const isError = computed(() => status.value === 'error' || result.value.startsWith('API Error'))
</script>

<template>
  <div class="mt-1">
    <button
      class="text-xs text-muted-foreground hover:text-foreground flex items-center gap-1.5 flex-wrap"
      @click="expanded = !expanded"
    >
      <span class="i-carbon-chevron-right w-3 h-3 transition-transform shrink-0" :class="{ 'rotate-90': expanded }" />
      <span class="i-carbon-bot w-3 h-3 shrink-0" :class="isError ? 'text-destructive/70' : ''" />
      <span :class="isError ? 'text-destructive/70' : ''">{{ summary || $t('block.taskNotification') }}</span>
      <span v-if="duration" class="opacity-50">{{ duration }}</span>
      <span v-if="tokens" class="opacity-50">{{ tokens }} tokens</span>
    </button>
    <pre v-if="expanded && result" class="mt-1 pl-3 border-l-2 border-default4/20 text-xs text-muted-foreground whitespace-pre-wrap break-all max-h-64 overflow-y-auto">{{ result }}</pre>
  </div>
</template>
