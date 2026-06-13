<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

const props = defineProps<{
  projects: Project[]
  loading: boolean
}>()

const BUCKETS = [
  { label: '< 10', max: 10 },
  { label: '10–50', max: 50 },
  { label: '50–200', max: 200 },
  { label: '200+', max: Infinity },
] as const

const distribution = computed(() => {
  const counts = new Array(BUCKETS.length).fill(0)
  let total = 0
  for (const p of props.projects) {
    for (const s of p.sessions) {
      const mc = s.message_count
      for (let i = 0; i < BUCKETS.length; i++) {
        if (mc < BUCKETS[i].max || i === BUCKETS.length - 1) {
          counts[i]++
          break
        }
      }
      total++
    }
  }
  return BUCKETS.map((b, i) => ({
    label: b.label,
    count: counts[i],
    pct: total > 0 ? (counts[i] / total) * 100 : 0,
  }))
})

const avgDepth = computed(() => {
  let sum = 0
  let count = 0
  for (const p of props.projects) {
    for (const s of p.sessions) {
      sum += s.message_count
      count++
    }
  }
  return count > 0 ? Math.round(sum / count) : 0
})
</script>

<template>
  <HomeCard icon="i-carbon-chart-histogram" title="会话深度" badge="按消息数">
    <div v-if="loading && !distribution.length" class="text-2xs text-muted-foreground py-2">加载中…</div>
    <template v-else>
      <div class="flex items-end gap-3 h-14 mt-1 px-2">
        <div v-for="d in distribution" :key="d.label" class="bucket">
          <div class="bucket-bar-wrap">
            <div class="bucket-bar" :style="{ height: `${Math.max(d.pct, 3)}%` }" />
          </div>
          <div class="bucket-label">{{ d.label }}</div>
          <div class="bucket-count">{{ d.count }}</div>
        </div>
      </div>
      <div class="text-2xs text-muted-foreground mt-2.5">
        平均每会话 <span class="text-foreground font-medium">{{ avgDepth }}</span> 条消息
      </div>
    </template>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
.bucket {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}
.bucket-bar-wrap {
  width: 100%;
  height: 48px;
  display: flex;
  align-items: flex-end;
}
.bucket-bar {
  width: 100%;
  border-radius: 2px 2px 0 0;
  background: color-mix(in oklch, var(--primary) 45%, var(--card));
  transition: height 0.2s;
}
.bucket:hover .bucket-bar {
  background: var(--primary);
}
.bucket-label {
  font-size: 10px;
  color: var(--muted-foreground);
  font-family: var(--font-mono, monospace);
}
.bucket-count {
  font-size: 11px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
</style>
