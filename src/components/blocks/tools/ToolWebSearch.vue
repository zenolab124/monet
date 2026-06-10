<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()

const query = computed(() => {
  const v = props.input.query
  return typeof v === 'string' ? v : ''
})

const allowedDomains = computed(() => {
  const v = props.input.allowed_domains
  return Array.isArray(v) ? v.filter(d => typeof d === 'string') as string[] : []
})

const blockedDomains = computed(() => {
  const v = props.input.blocked_domains
  return Array.isArray(v) ? v.filter(d => typeof d === 'string') as string[] : []
})
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5 flex-wrap">
      <span class="i-carbon-search-locate w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">WebSearch</span>
      <code v-if="query" class="px-1.5 py-0.5 rounded border border-border text-muted-foreground font-mono">{{ query }}</code>
    </div>
    <div v-if="allowedDomains.length > 0" class="mt-1 flex items-center gap-1 flex-wrap">
      <span class="text-muted-foreground">允许：</span>
      <span v-for="d in allowedDomains" :key="`a-${d}`" class="px-1.5 py-0.5 rounded border border-primary/40 text-primary font-mono">{{ d }}</span>
    </div>
    <div v-if="blockedDomains.length > 0" class="mt-1 flex items-center gap-1 flex-wrap">
      <span class="text-muted-foreground">屏蔽：</span>
      <span v-for="d in blockedDomains" :key="`b-${d}`" class="px-1.5 py-0.5 rounded border border-destructive/40 text-destructive font-mono">{{ d }}</span>
    </div>
  </div>
</template>
