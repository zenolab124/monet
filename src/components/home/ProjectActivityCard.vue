<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Project } from '../../types'
import { fileName } from '@/utils/path'
import DashboardSection from './DashboardSection.vue'

const { t } = useI18n()

const props = defineProps<{
  projects: Project[]
  loading: boolean
}>()

const rows = computed(() => {
  const now = Date.now() / 1000
  const sorted = props.projects
    .filter(p => p.last_active !== null)
    .sort((a, b) => (b.last_active ?? 0) - (a.last_active ?? 0))
    .slice(0, 8)
  const max = Math.max(...sorted.map(p => p.session_count), 1)
  return sorted.map(p => ({
    id: p.id,
    name: projectName(p.display_path),
    path: p.display_path,
    sessions: p.session_count,
    pct: (p.session_count / max) * 100,
    recency: timeAgo(p.last_active ?? 0, now),
  }))
})

function projectName(path: string): string {
  return fileName(path)
}

function timeAgo(ts: number, now: number): string {
  const diff = Math.floor(now - ts)
  if (diff < 60) return t('time.justNow')
  if (diff < 3600) return t('time.minutesAgo', { n: Math.floor(diff / 60) })
  if (diff < 86400) return t('time.hoursAgo', { n: Math.floor(diff / 3600) })
  if (diff < 604800) return t('time.daysAgo', { n: Math.floor(diff / 86400) })
  return t('time.weeksAgo', { n: Math.floor(diff / 604800) })
}
</script>

<template>
  <DashboardSection icon="i-carbon-folder" :title="$t('home.projectActivity.title')" :badge="$t('home.projectActivity.badge')">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('home.projectActivity.noProjects') }}</div>
    <div v-else class="bar-list">
      <div v-for="r in rows" :key="r.id" class="bar-row" :title="r.path">
        <div class="bar-meta">
          <span class="bar-name">{{ r.name }}</span>
          <span class="bar-info">{{ $t('home.projectActivity.nSessions', { count: r.sessions }) }} · {{ r.recency }}</span>
        </div>
        <div class="bar-track">
          <div class="bar-fill" :style="{ width: `${r.pct}%` }" />
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
  gap: 8px;
}
.bar-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.bar-meta {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.bar-name {
  font-size: 11px;
  color: var(--foreground);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bar-info {
  font-size: 10px;
  color: var(--muted-foreground);
  font-variant-numeric: tabular-nums;
  margin-left: auto;
  flex-shrink: 0;
}
.bar-track {
  height: 6px;
  border-radius: 3px;
  background: var(--muted);
  overflow: hidden;
}
.bar-fill {
  height: 100%;
  border-radius: 3px;
  background: color-mix(in oklch, var(--primary) 55%, var(--card));
}
</style>
