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

const emit = defineEmits<{ 'go-session': [sessionId: string] }>()

const recent = computed(() => {
  const all = props.projects.flatMap(p =>
    p.sessions.map(s => ({ ...s, projectPath: p.display_path })),
  )
  all.sort((a, b) => b.last_modified - a.last_modified)
  return all.slice(0, 8)
})

function timeAgo(ts: number): string {
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return t('time.justNow')
  if (diff < 3600) return t('time.minutesAgo', { n: Math.floor(diff / 60) })
  if (diff < 86400) return t('time.hoursAgo', { n: Math.floor(diff / 3600) })
  if (diff < 604800) return t('time.daysAgo', { n: Math.floor(diff / 86400) })
  return t('time.weeksAgo', { n: Math.floor(diff / 604800) })
}

function projectName(path: string): string {
  const parts = path.split('/')
  return parts[parts.length - 1] || path
}
</script>

<template>
  <HomeCard icon="i-carbon-recently-viewed" :title="$t('home.recentSessions.title')" :badge="$t('home.recentSessions.badge')" wide>
    <div v-if="loading && !recent.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <div v-else-if="!recent.length" class="text-2xs text-muted-foreground py-2">{{ $t('home.recentSessions.noSessions') }}</div>
    <div v-else class="flex flex-col">
      <button
        v-for="s in recent"
        :key="s.id"
        class="session-row"
        @click="emit('go-session', s.id)"
      >
        <span class="flex-1 min-w-0 truncate text-xs">{{ s.title || s.first_user_message || $t('archive.noTitle') }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground font-mono">{{ projectName(s.projectPath) }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums text-right">{{ timeAgo(s.last_modified) }}</span>
      </button>
    </div>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}

.session-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 4px;
  border-radius: var(--radius);
  cursor: pointer;
  text-align: left;
  background: transparent;
  border: none;
  color: var(--foreground);
}
.session-row:hover {
  background: var(--muted);
}
.session-row + .session-row {
  border-top: 1px solid color-mix(in oklch, var(--border) 50%, transparent);
}
</style>
