<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

const props = defineProps<{
  projects: Project[]
  loading: boolean
}>()

const hours = computed(() => {
  const counts = new Array(24).fill(0)
  for (const p of props.projects) {
    for (const s of p.sessions) {
      if (!s.last_modified) continue
      const h = new Date(s.last_modified * 1000).getHours()
      counts[h]++
    }
  }
  const max = Math.max(...counts, 1)
  return counts.map((c, h) => ({ hour: h, count: c, pct: (c / max) * 100 }))
})

const peakHour = computed(() => {
  let maxH = 0
  for (let i = 1; i < hours.value.length; i++) {
    if (hours.value[i].count > hours.value[maxH].count) maxH = i
  }
  return hours.value[maxH].count > 0 ? maxH : null
})
</script>

<template>
  <HomeCard icon="i-carbon-time" :title="$t('home.workRhythm.title')" :badge="$t('home.workRhythm.badge')">
    <div v-if="loading && !hours.length" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <template v-else>
      <div class="flex items-end gap-px h-12 mt-1">
        <div
          v-for="h in hours"
          :key="h.hour"
          class="bar-col"
          :title="$t('home.workRhythm.barTitle', { hour: h.hour, count: h.count })"
        >
          <div class="bar" :class="h.hour === peakHour ? 'peak' : ''" :style="{ height: `${Math.max(h.pct, 2)}%` }" />
        </div>
      </div>
      <div class="flex justify-between mt-1 text-2xs text-muted-foreground tabular-nums">
        <span>{{ $t('home.workRhythm.hour0') }}</span>
        <span>{{ $t('home.workRhythm.hour6') }}</span>
        <span>{{ $t('home.workRhythm.hour12') }}</span>
        <span>{{ $t('home.workRhythm.hour18') }}</span>
        <span>{{ $t('home.workRhythm.hour24') }}</span>
      </div>
      <div v-if="peakHour !== null" class="text-2xs text-muted-foreground mt-1.5">
        {{ $t('home.workRhythm.peakHour', { start: peakHour, end: peakHour + 1 }) }}
      </div>
    </template>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
.bar-col {
  flex: 1;
  display: flex;
  align-items: flex-end;
  min-width: 0;
}
.bar {
  width: 100%;
  border-radius: 1px 1px 0 0;
  background: color-mix(in oklch, var(--primary) 50%, var(--card));
  transition: height 0.2s;
}
.bar.peak {
  background: var(--primary);
}
.bar-col:hover .bar {
  background: var(--primary);
}
</style>
