<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

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
    top.push(['其他', rest.reduce((s, [, c]) => s + c, 0)])
  }
  return top.map(([name, count]) => ({
    name,
    count,
    pct: total > 0 ? (count / total) * 100 : 0,
  }))
})
</script>

<template>
  <HomeCard icon="i-carbon-model-alt" title="模型偏好" badge="按会话数">
    <div v-if="loading && !rows.length" class="text-2xs text-muted-foreground py-2">加载中…</div>
    <div v-else-if="!rows.length" class="text-2xs text-muted-foreground py-2">暂无数据</div>
    <div v-else class="flex flex-col gap-1.25 mt-0.5">
      <div v-for="m in rows" :key="m.name" class="flex items-center gap-2 text-xs">
        <span class="w-20 text-muted-foreground font-mono text-2xs truncate" :title="m.name">{{ m.name }}</span>
        <span class="flex-1 h-1.25 rounded-sm bg-muted overflow-hidden">
          <span class="block h-full rounded-sm bg-primary/70" :style="{ width: `${m.pct}%` }" />
        </span>
        <span class="w-10 text-right tabular-nums text-muted-foreground text-2xs">{{ m.count }} 次</span>
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
