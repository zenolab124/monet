<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import DashboardSection from './DashboardSection.vue'

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
  <DashboardSection icon="i-carbon-chart-histogram" :title="$t('home.sessionDepth.title')" :badge="$t('home.sessionDepth.badge')">
    <div v-if="loading && !distribution.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <template v-else>
      <div class="depth-chart">
        <div v-for="d in distribution" :key="d.label" class="bucket">
          <div class="bucket-bar-wrap">
            <div class="bucket-bar" :style="{ height: `${Math.max(d.pct, 3)}%` }">
              <span v-if="d.count > 0" class="bucket-count-inner">{{ d.count }}</span>
            </div>
          </div>
          <div class="bucket-label">{{ d.label }}</div>
        </div>
      </div>
      <div class="text-2xs text-muted-foreground mt-3">
        {{ $t('home.sessionDepth.avgMessages', { avg: avgDepth }) }}
      </div>
    </template>
  </DashboardSection>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
.depth-chart {
  display: flex;
  gap: 12px;
  height: 140px;
  padding: 0 8px;
}
.bucket {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}
.bucket-bar-wrap {
  width: 100%;
  flex: 1;
  display: flex;
  align-items: flex-end;
}
.bucket-bar {
  width: 100%;
  border-radius: 3px 3px 0 0;
  background: color-mix(in oklch, var(--primary) 45%, var(--card));
  transition: height 0.2s;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 4px;
  min-height: 20px;
}
.bucket:hover .bucket-bar {
  background: var(--primary);
}
.bucket-count-inner {
  font-size: 11px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--card);
}
.bucket-label {
  font-size: 11px;
  color: var(--muted-foreground);
  font-family: var(--font-mono, monospace);
}
</style>
