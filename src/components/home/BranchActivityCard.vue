<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

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
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)}分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)}小时前`
  if (diff < 604800) return `${Math.floor(diff / 86400)}天前`
  return `${Math.floor(diff / 604800)}周前`
}
</script>

<template>
  <HomeCard icon="i-carbon-branch" title="分支活跃" badge="按最近活跃">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">加载中…</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">暂无分支数据</div>
    <div v-else class="flex flex-col">
      <div v-for="r in rows" :key="r.branch" class="branch-row">
        <span class="i-carbon-branch w-3 h-3 text-muted-foreground shrink-0" />
        <span class="flex-1 min-w-0 truncate text-xs font-mono" :title="r.branch">{{ r.branch }}</span>
        <span class="shrink-0 text-2xs text-muted-foreground tabular-nums">{{ r.count }} 会话</span>
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
