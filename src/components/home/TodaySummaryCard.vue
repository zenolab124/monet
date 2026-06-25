<script setup lang="ts">
import { computed } from 'vue'
import type { UsageStats, Project } from '../../types'
import { formatTokens } from '../../types'
import HomeCard from './HomeCard.vue'

const props = defineProps<{
  usage: UsageStats | null
  projects: Project[]
  loading: boolean
}>()

function fmtToday(): string {
  const d = new Date()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${m}-${day}`
}

const todayDate = fmtToday()

const todayTokens = computed(() => {
  if (!props.usage) return 0
  return props.usage.daily.find(d => d.date === todayDate)?.total ?? 0
})

const todaySessions = computed(() => {
  const startOfDay = new Date()
  startOfDay.setHours(0, 0, 0, 0)
  const ts = startOfDay.getTime() / 1000
  let count = 0
  for (const p of props.projects) {
    for (const s of p.sessions) {
      if (s.last_modified >= ts) count++
    }
  }
  return count
})

const todayModels = computed(() => {
  const startOfDay = new Date()
  startOfDay.setHours(0, 0, 0, 0)
  const ts = startOfDay.getTime() / 1000
  const models = new Set<string>()
  for (const p of props.projects) {
    for (const s of p.sessions) {
      if (s.last_modified >= ts && s.model) models.add(s.model.replace(/^claude-/, ''))
    }
  }
  return [...models]
})
</script>

<template>
  <HomeCard icon="i-carbon-calendar" :title="$t('home.todaySummary.title')" :badge="$t('home.todaySummary.badge')">
    <div v-if="loading && !props.usage" class="text-2xs text-muted-foreground py-2">{{ $t('common.loading') }}</div>
    <template v-else>
      <div class="stat-body">
        <div class="stat-row">
          <span class="stat-label">{{ $t('common.sessions') }}</span>
          <span class="stat-value">{{ todaySessions }}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Token</span>
          <span class="stat-value">{{ formatTokens(todayTokens) }}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">{{ $t('common.model') }}</span>
          <span v-if="todayModels.length" class="stat-models">
            <span v-for="m in todayModels" :key="m" class="model-tag">{{ m }}</span>
          </span>
          <span v-else class="stat-value text-muted-foreground">—</span>
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
.stat-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
  justify-content: center;
  height: 100%;
}
.stat-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.stat-label {
  font-size: 11px;
  color: var(--muted-foreground);
  min-width: 40px;
  flex-shrink: 0;
}
.stat-value {
  font-size: 18px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.stat-models {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.model-tag {
  font-size: 10px;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--muted-foreground);
  font-family: var(--font-mono, monospace);
}
</style>
