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
  const now = Date.now() / 1000
  return props.projects
    .filter(p => p.last_active !== null)
    .sort((a, b) => (b.last_active ?? 0) - (a.last_active ?? 0))
    .slice(0, 6)
    .map(p => ({
      id: p.id,
      name: projectName(p.display_path),
      path: p.display_path,
      sessions: p.session_count,
      recency: timeAgo(p.last_active ?? 0, now),
    }))
})

function projectName(path: string): string {
  const parts = path.split('/')
  return parts[parts.length - 1] || path
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
  <HomeCard icon="i-carbon-folder" :title="$t('home.projectActivity.title')" :badge="$t('home.projectActivity.badge')">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">{{ $t('home.projectActivity.noProjects') }}</div>
    <div v-else class="flex flex-col gap-1.5">
      <div v-for="r in rows" :key="r.id" class="flex items-center gap-2 text-xs">
        <span class="flex-1 min-w-0 truncate" :title="r.path">{{ r.name }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums">{{ $t('home.projectActivity.nSessions', { count: r.sessions }) }}</span>
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
</style>
