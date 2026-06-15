<script setup lang="ts">
import { computed } from 'vue'
import type { UsageStats } from '../../types'
import HomeCard from './HomeCard.vue'

const props = defineProps<{
  usage: UsageStats | null
  loading: boolean
}>()

function fmtDate(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${m}-${day}`
}

const streaks = computed(() => {
  if (!props.usage) return { current: 0, longest: 0 }
  const activeDays = new Set(props.usage.daily.filter(d => d.total > 0).map(d => d.date))

  const today = new Date()
  today.setHours(0, 0, 0, 0)

  let current = 0
  const d = new Date(today)
  if (!activeDays.has(fmtDate(d))) {
    d.setDate(d.getDate() - 1)
  }
  while (activeDays.has(fmtDate(d))) {
    current++
    d.setDate(d.getDate() - 1)
  }

  const sorted = [...activeDays].sort()
  let longest = 0
  let run = 0
  let prev = ''
  for (const day of sorted) {
    if (prev) {
      const p = new Date(prev)
      p.setDate(p.getDate() + 1)
      if (fmtDate(p) === day) {
        run++
      } else {
        run = 1
      }
    } else {
      run = 1
    }
    if (run > longest) longest = run
    prev = day
  }

  return { current, longest }
})

const activeDays = computed(() => {
  if (!props.usage) return 0
  return props.usage.daily.filter(d => d.total > 0).length
})
</script>

<template>
  <HomeCard icon="i-carbon-fire" :title="$t('home.streak.title')" :badge="$t('home.streak.badge')">
    <div v-if="loading" class="text-2xs text-muted-foreground py-2">{{ $t('home.streak.counting') }}</div>
    <template v-else>
      <div class="flex gap-6 mt-0.5">
        <div class="streak-stat">
          <div class="streak-num">{{ streaks.current }}</div>
          <div class="streak-label">{{ $t('home.streak.currentStreak') }}</div>
        </div>
        <div class="streak-stat">
          <div class="streak-num best">{{ streaks.longest }}</div>
          <div class="streak-label">{{ $t('home.streak.longestStreak') }}</div>
        </div>
        <div class="streak-stat">
          <div class="streak-num">{{ activeDays }}</div>
          <div class="streak-label">{{ $t('home.streak.activeDays') }}</div>
        </div>
      </div>
    </template>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
.streak-stat {
  text-align: center;
}
.streak-num {
  font-size: 26px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
}
.streak-num.best {
  color: var(--accent);
}
.streak-label {
  font-size: 10px;
  color: var(--muted-foreground);
  margin-top: 2px;
}
</style>
