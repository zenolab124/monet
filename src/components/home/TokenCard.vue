<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { UsageStats } from '../../types'
import { formatTokens } from '../../types'
import DashboardSection from './DashboardSection.vue'

const { t } = useI18n()

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

const CHART_W = 800
const CHART_H = 160
const PAD = { top: 8, right: 0, bottom: 20, left: 48 }
const plotW = CHART_W - PAD.left - PAD.right
const plotH = CHART_H - PAD.top - PAD.bottom

const chartData = computed(() => {
  if (!props.usage?.daily.length) return null
  const sorted = [...props.usage.daily].sort((a, b) => a.date.localeCompare(b.date))
  const recent = sorted.slice(-30)
  if (!recent.length) return null
  const max = Math.max(...recent.map(d => d.total), 1)
  const points = recent.map((d, i) => ({
    x: PAD.left + (i / Math.max(recent.length - 1, 1)) * plotW,
    y: PAD.top + plotH - (d.total / max) * plotH,
    date: d.date,
    total: d.total,
  }))
  const linePath = points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x},${p.y}`).join(' ')
  const areaPath = `${linePath} L${points[points.length - 1].x},${PAD.top + plotH} L${points[0].x},${PAD.top + plotH} Z`

  const yTicks = [0, Math.round(max / 2), max].map(v => ({
    y: PAD.top + plotH - (v / max) * plotH,
    label: formatTokens(v),
  }))

  const step = Math.max(1, Math.floor(recent.length / 5))
  const xTicks = recent
    .filter((_, i) => i % step === 0 || i === recent.length - 1)
    .map((d, _, arr) => ({
      x: PAD.left + (recent.indexOf(d) / Math.max(recent.length - 1, 1)) * plotW,
      label: d.date.slice(5),
    }))

  return { points, linePath, areaPath, yTicks, xTicks, max }
})

const modelRows = computed(() => {
  if (!props.usage) return []
  const { total, byModel } = props.usage.month
  const list = byModel.slice(0, 5).map(m => ({ name: m.model, total: m.total }))
  const rest = byModel.slice(5)
  if (rest.length)
    list.push({ name: t('common.other'), total: rest.reduce((s, m) => s + m.total, 0) })
  return list.map(r => ({
    name: r.name,
    amount: formatTokens(r.total),
    pct: total > 0 ? (r.total / total) * 100 : 0,
  }))
})
</script>

<template>
  <DashboardSection icon="i-carbon-meter" :title="$t('home.token.title')" :badge="$t('home.token.badge')">
    <template v-if="error">
      <div class="py-3 text-xs text-muted-foreground">{{ $t('common.loadFailed') }}</div>
      <button class="retry-btn" @click="emit('retry')">{{ $t('common.retry') }}</button>
    </template>
    <template v-else>
      <div class="token-header">
        <div class="big-num">
          {{ loading ? '—' : formatTokens(usage?.month.total ?? 0) }}<small>tokens</small>
          <span v-if="trend && !loading" class="trend" :class="trend.up ? 'up' : 'down'">
            {{ trend.label }}{{ trend.pct }}%
          </span>
        </div>
      </div>

      <!-- 面积图 -->
      <div v-if="chartData && !loading" class="chart-wrap">
        <svg :viewBox="`0 0 ${CHART_W} ${CHART_H}`" class="chart-svg">
          <defs>
            <linearGradient id="areaGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="var(--primary)" stop-opacity="0.25" />
              <stop offset="100%" stop-color="var(--primary)" stop-opacity="0.02" />
            </linearGradient>
          </defs>
          <!-- Y 轴刻度线 -->
          <template v-for="tick in chartData.yTicks" :key="tick.label">
            <line :x1="PAD.left" :y1="tick.y" :x2="CHART_W" :y2="tick.y" stroke="var(--border)" stroke-width="0.5" />
            <text :x="PAD.left - 6" :y="tick.y + 3" text-anchor="end" class="axis-label">{{ tick.label }}</text>
          </template>
          <!-- X 轴刻度 -->
          <text v-for="tick in chartData.xTicks" :key="tick.label" :x="tick.x" :y="CHART_H - 4" text-anchor="middle" class="axis-label">{{ tick.label }}</text>
          <!-- 面积填充 -->
          <path :d="chartData.areaPath" fill="url(#areaGrad)" />
          <!-- 折线 -->
          <path :d="chartData.linePath" fill="none" stroke="var(--primary)" stroke-width="1.5" stroke-linejoin="round" />
          <!-- 数据点 -->
          <circle
            v-for="p in chartData.points"
            :key="p.date"
            :cx="p.x" :cy="p.y" r="2"
            fill="var(--primary)"
            opacity="0"
            class="data-dot"
          >
            <title>{{ p.date }} · {{ formatTokens(p.total) }}</title>
          </circle>
        </svg>
      </div>
      <div v-else-if="loading" class="chart-placeholder text-2xs text-muted-foreground">
        {{ $t('home.token.counting') }}
      </div>

      <!-- 模型分布条 -->
      <div v-if="!loading && modelRows.length" class="model-bars">
        <div v-for="m in modelRows" :key="m.name" class="bar-row">
          <span class="bar-label" :title="m.name">{{ m.name }}</span>
          <span class="bar-track">
            <span class="bar-fill" :style="{ width: `${m.pct}%` }" />
          </span>
          <span class="bar-value">{{ m.amount }}</span>
        </div>
      </div>
    </template>
  </DashboardSection>
</template>

<style scoped>
.token-header {
  margin-bottom: 12px;
}
.big-num {
  font-size: 28px;
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

.chart-wrap {
  margin: 0 -4px;
}
.chart-svg {
  width: 100%;
  height: auto;
}
.axis-label {
  font-size: 9px;
  fill: var(--muted-foreground);
  font-variant-numeric: tabular-nums;
}
.data-dot:hover {
  opacity: 1 !important;
  r: 3.5;
}

.chart-placeholder {
  height: 160px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.model-bars {
  margin-top: 16px;
  display: flex;
  flex-direction: column;
  gap: 5px;
  border-top: 1px solid var(--border);
  padding-top: 12px;
}
.bar-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.bar-label {
  flex-shrink: 0;
  max-width: 30%;
  color: var(--muted-foreground);
  font-family: var(--font-mono, monospace);
  font-size: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bar-track {
  flex: 1;
  height: 6px;
  border-radius: 3px;
  background: var(--muted);
  overflow: hidden;
  min-width: 0;
}
.bar-fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--primary);
}
.bar-value {
  flex-shrink: 0;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--muted-foreground);
  font-size: 10px;
}

.text-2xs {
  font-size: 10px;
  line-height: 1.4;
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
