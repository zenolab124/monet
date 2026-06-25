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
  const counts = new Map<string, { count: number; lastActive: number }>()
  for (const p of props.projects) {
    for (const s of p.sessions) {
      if (!s.git_branch) continue
      const prev = counts.get(s.git_branch)
      if (!prev) {
        counts.set(s.git_branch, { count: 1, lastActive: s.last_modified })
      } else {
        prev.count++
        if (s.last_modified > prev.lastActive) prev.lastActive = s.last_modified
      }
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1].lastActive - a[1].lastActive)
    .slice(0, 8)
    .map(([branch, data]) => ({
      branch,
      count: data.count,
      recency: timeAgo(data.lastActive),
    }))
})

function timeAgo(ts: number): string {
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return t('time.justNow')
  if (diff < 3600) return t('time.minutesAgo', { n: Math.floor(diff / 60) })
  if (diff < 86400) return t('time.hoursAgo', { n: Math.floor(diff / 3600) })
  if (diff < 604800) return t('time.daysAgo', { n: Math.floor(diff / 86400) })
  return t('time.weeksAgo', { n: Math.floor(diff / 604800) })
}
</script>

<template>
  <HomeCard icon="i-carbon-branch" :title="$t('home.branchActivity.title')" :badge="$t('home.branchActivity.badge')">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('home.branchActivity.noBranches') }}</div>
    <div v-else class="flex flex-col">
      <div v-for="r in rows" :key="r.branch" class="branch-row">
        <span class="i-carbon-branch w-3 h-3 text-muted-foreground shrink-0" />
        <span class="flex-1 min-w-0 truncate text-xs font-mono" :title="r.branch">{{ r.branch }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums">{{ $t('home.branchActivity.nSessions', { count: r.count }) }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums text-right">{{ r.recency }}</span>
      </div>
    </div>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
.branch-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 2px;
}
.branch-row + .branch-row {
  border-top: 1px solid color-mix(in oklch, var(--border) 50%, transparent);
}
</style>
