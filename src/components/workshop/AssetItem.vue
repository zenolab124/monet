<script setup lang="ts">
withDefaults(defineProps<{
  icon: string
  name: string
  version?: string | null
  description: string
  descTitle?: string
  source: string
  dimmed?: boolean
}>(), { version: null, descTitle: undefined, dimmed: false })

function scopeType(s: string) {
  if (s === '全局' || s === 'Global') return 'global'
  if (s.startsWith('project')) return 'project'
  return 'user'
}
</script>

<template>
  <div class="fitem" :class="{ 'opacity-60': dimmed }">
    <div class="fi-head">
      <div class="fi-name">
        <span class="truncate">{{ name }}</span>
        <span v-if="version" class="fi-ver">v{{ version }}</span>
      </div>
      <span class="fi-src" :class="`fi-src--${scopeType(source)}`">{{ source }}</span>
    </div>
    <div class="fi-body">
      <div class="fi-desc" :title="descTitle ?? description">{{ description }}</div>
      <slot />
    </div>
  </div>
</template>

<style scoped>
.fitem {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
  box-shadow: var(--shadow-paper);
  transition: box-shadow 0.15s, transform 0.15s;
}
.fitem:hover {
  box-shadow: 0 2px 6px rgba(0,0,0,0.1);
  transform: translateY(-1px);
}
.fi-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.fi-name {
  font-size: 12.5px;
  font-weight: 600;
  display: flex;
  align-items: baseline;
  gap: 5px;
  min-width: 0;
  flex: 1;
}
.fi-ver {
  font-size: 10px;
  font-family: var(--font-mono);
  font-weight: 400;
  color: var(--muted-foreground);
}
.fi-body {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  min-width: 0;
}
.fi-desc {
  flex: 1;
  font-size: 11px;
  line-height: 1.5;
  color: var(--muted-foreground);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.fi-src {
  font-size: 10px;
  border-radius: 3px;
  padding: 1px 6px;
  flex-shrink: 0;
  line-height: 1.4;
}
.fi-src--global {
  background: color-mix(in srgb, var(--success, #2d7d3a) 12%, transparent);
  color: var(--success, #2d7d3a);
}
.fi-src--project {
  background: color-mix(in srgb, var(--primary) 10%, transparent);
  color: var(--primary);
}
.fi-src--user {
  background: color-mix(in srgb, var(--accent, #c47a20) 12%, transparent);
  color: var(--accent, #c47a20);
}
</style>
