<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

const { t } = useI18n()

const props = defineProps<{
  projects: Project[]
  loading: boolean
}>()

const rows = computed(() => {
  const counts = new Map<string, number>()
  for (const p of props.projects) {
    for (const s of p.sessions) {
      if (!s.model) continue
      const name = s.model.replace(/^claude-/, '')
      counts.set(name, (counts.get(name) ?? 0) + 1)
    }
  }
  const sorted = [...counts.entries()].sort((a, b) => b[1] - a[1])
  const total = sorted.reduce((s, [, c]) => s + c, 0)
  const top = sorted.slice(0, 5)
  const rest = sorted.slice(5)
  if (rest.length) {
    top.push([t('common.other'), rest.reduce((s, [, c]) => s + c, 0)])
  }
  return top.map(([name, count]) => ({
    name,
    count,
    pct: total > 0 ? (count / total) * 100 : 0,
  }))
})
</script>

<template>
  <HomeCard icon="i-carbon-model-alt" :title="$t('home.modelPreference.title')" :badge="$t('home.modelPreference.badge')">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.noData') }}</div>
    <div v-else class="flex flex-col gap-1.25 mt-0.5">
      <div v-for="m in rows" :key="m.name" class="bar-row">
        <span class="bar-label" :title="m.name">{{ m.name }}</span>
        <span class="bar-track">
          <span class="bar-fill" :style="{ width: `${m.pct}%` }" />
        </span>
        <span class="bar-value">{{ $t('home.modelPreference.nTimes', { count: m.count }) }}</span>
      </div>
    </div>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}

.bar-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.bar-label {
  flex-shrink: 0;
  max-width: 35%;
  color: var(--muted-foreground);
  font-family: var(--font-mono, monospace);
  font-size: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bar-track {
  flex: 1;
  height: 5px;
  border-radius: 2px;
  background: var(--muted);
  overflow: hidden;
  min-width: 0;
}
.bar-fill {
  display: block;
  height: 100%;
  border-radius: 2px;
  background: color-mix(in oklch, var(--primary) 70%, var(--card));
}
.bar-value {
  flex-shrink: 0;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--muted-foreground);
  font-size: 10px;
}
</style>
