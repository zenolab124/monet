<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { UsageStats } from '../../types'
import { formatTokens } from '../../types'
import HomeCard from './HomeCard.vue'

const { t } = useI18n()

/** Token 消耗卡（v2.2.0 FR-002）：本月总量 + 前 5 模型条形分布，其余合并「其他」 */
const props = defineProps<{
  usage: UsageStats | null
  loading: boolean
  error: string | null
}>()

const emit = defineEmits<{ retry: [] }>()

const lastMonthTotal = computed(() => {
  if (!props.usage) return 0
  const now = new Date()
  const lm = now.getMonth() === 0 ? 11 : now.getMonth() - 1
  const ly = now.getMonth() === 0 ? now.getFullYear() - 1 : now.getFullYear()
  return props.usage.daily
    .filter(d => {
      const [y, m] = d.date.split('-').map(Number)
      return y === ly && m === lm + 1
    })
    .reduce((sum, d) => sum + d.total, 0)
})

const trend = computed(() => {
  const curr = props.usage?.month.total ?? 0
  const prev = lastMonthTotal.value
  if (prev === 0) return null
  const pct = Math.round(((curr - prev) / prev) * 100)
  return { pct: Math.abs(pct), up: pct >= 0, label: pct >= 0 ? '↑' : '↓' }
})

const rows = computed(() => {
  if (!props.usage) return []
  const { total, byModel } = props.usage.month
  const list = byModel.slice(0, 5).map((m) => ({ name: m.model, total: m.total }))
  const rest = byModel.slice(5)
  if (rest.length) {
    list.push({ name: t('common.other'), total: rest.reduce((s, m) => s + m.total, 0) })
  }
  return list.map((r) => ({
    name: r.name,
    amount: formatTokens(r.total),
    pct: total > 0 ? (r.total / total) * 100 : 0,
  }))
})
</script>

<template>
  <HomeCard icon="i-carbon-meter" :title="$t('home.token.title')" :badge="$t('home.token.badge')">
    <template v-if="error">
      <div class="py-3 text-xs text-muted-foreground">{{ $t('common.loadFailed') }}</div>
      <button class="retry-btn" @click="emit('retry')">{{ $t('common.retry') }}</button>
    </template>
    <template v-else>
      <div class="big-num">
        {{ loading ? '—' : formatTokens(usage?.month.total ?? 0) }}<small>tokens</small>
        <span v-if="trend && !loading" class="trend" :class="trend.up ? 'up' : 'down'">
          {{ trend.label }}{{ trend.pct }}%
        </span>
      </div>
      <div class="mt-2.5 flex flex-col gap-1.25">
        <div v-if="loading" class="text-2xs text-muted-foreground">{{ $t('home.token.counting') }}</div>
        <div v-else-if="!rows.length" class="text-2xs text-muted-foreground">{{ $t('home.token.noUsage') }}</div>
        <div v-for="m in rows" :key="m.name" class="bar-row">
          <span class="bar-label" :title="m.name">{{ m.name }}</span>
          <span class="bar-track">
            <span class="bar-fill bg-primary" :style="{ width: `${m.pct}%` }" />
          </span>
          <span class="bar-value">{{ m.amount }}</span>
        </div>
      </div>
    </template>
  </HomeCard>
</template>

<style scoped>
.big-num {
  font-size: 26px;
  font-weight: 600;
  letter-spacing: -0.02em;
  font-variant-numeric: tabular-nums;
}
.big-num small {
  font-size: 12px;
  font-weight: 400;
  color: var(--muted-foreground);
  margin-left: 4px;
}
.trend {
  font-size: 11px;
  font-weight: 500;
  margin-left: 6px;
}
.trend.up { color: var(--accent); }
.trend.down { color: var(--primary); }

.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}

.bar-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}
.bar-label {
  flex-shrink: 0;
  max-width: 35%;
  color: var(--muted-foreground);
  font-family: var(--font-mono, monospace);
  font-size: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bar-track {
  flex: 1;
  height: 5px;
  border-radius: 2px;
  background: var(--muted);
  overflow: hidden;
  min-width: 0;
}
.bar-fill {
  display: block;
  height: 100%;
  border-radius: 2px;
}
.bar-value {
  flex-shrink: 0;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--muted-foreground);
  font-size: 10px;
}

.retry-btn {
  font-size: 11px;
  padding: 2px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--foreground);
  background: var(--card);
  cursor: pointer;
}
.retry-btn:hover {
  background: var(--muted);
}
</style>
