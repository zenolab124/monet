<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

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
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)}分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)}小时前`
  if (diff < 604800) return `${Math.floor(diff / 86400)}天前`
  return `${Math.floor(diff / 604800)}周前`
}
</script>

<template>
  <HomeCard icon="i-carbon-folder" title="项目活跃" badge="全部">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">加载中…</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">暂无项目</div>
    <div v-else class="flex flex-col gap-1.5">
      <div v-for="r in rows" :key="r.id" class="flex items-center gap-2 text-xs">
        <span class="flex-1 min-w-0 truncate" :title="r.path">{{ r.name }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums">{{ r.sessions }} 会话</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums w-14 text-right">{{ r.recency }}</span>
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
