<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Project } from '../../types'
import DashboardSection from './DashboardSection.vue'

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
  const top = sorted.slice(0, 6)
  const rest = sorted.slice(6)
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
  <DashboardSection icon="i-carbon-model-alt" :title="$t('home.modelPreference.title')" :badge="$t('home.modelPreference.badge')">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.noData') }}</div>
    <div v-else class="bar-list">
      <div v-for="m in rows" :key="m.name" class="bar-row">
        <span class="bar-label" :title="m.name">{{ m.name }}</span>
        <div class="bar-body">
          <span class="bar-track">
            <span class="bar-fill" :style="{ width: `${m.pct}%` }" />
          </span>
          <span class="bar-value">{{ $t('home.modelPreference.nTimes', { count: m.count }) }}</span>
        </div>
      </div>
    </div>
  </DashboardSection>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
.bar-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.bar-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.bar-label {
  color: var(--foreground);
  font-family: var(--font-mono, monospace);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bar-body {
  display: flex;
  align-items: center;
  gap: 8px;
}
.bar-track {
  flex: 1;
  height: 8px;
  border-radius: 4px;
  background: var(--muted);
  overflow: hidden;
  min-width: 0;
}
.bar-fill {
  display: block;
  height: 100%;
  border-radius: 4px;
  background: color-mix(in oklch, var(--primary) 70%, var(--card));
}
.bar-value {
  flex-shrink: 0;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--muted-foreground);
  font-size: 10px;
  min-width: 40px;
}
</style>
