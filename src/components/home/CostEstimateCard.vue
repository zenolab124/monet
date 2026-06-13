<script setup lang="ts">
import { computed } from 'vue'
import type { Project } from '../../types'
import HomeCard from './HomeCard.vue'

const props = defineProps<{
  projects: Project[]
  loading: boolean
}>()

const PRICING: Record<string, { input: number; output: number }> = {
  'opus': { input: 15, output: 75 },
  'sonnet': { input: 3, output: 15 },
  'haiku': { input: 0.80, output: 4 },
}

function matchPricing(model: string | null): { input: number; output: number } | null {
  if (!model) return null
  const lower = model.toLowerCase()
  for (const [key, price] of Object.entries(PRICING)) {
    if (lower.includes(key)) return price
  }
  return null
}

const estimate = computed(() => {
  if (!props.projects.length) return null
  const now = new Date()
  const monthStart = new Date(now.getFullYear(), now.getMonth(), 1).getTime() / 1000

  let totalCost = 0
  const byModel = new Map<string, number>()

  for (const p of props.projects) {
    for (const s of p.sessions) {
      if (s.last_modified < monthStart) continue
      const pricing = matchPricing(s.model)
      if (!pricing) continue
      const t = s.total_tokens
      const cost = (t.input_tokens + t.cache_creation_input_tokens) * pricing.input / 1_000_000
        + t.output_tokens * pricing.output / 1_000_000
        + t.cache_read_input_tokens * pricing.input * 0.1 / 1_000_000
      totalCost += cost
      const name = s.model ?? 'unknown'
      byModel.set(name, (byModel.get(name) ?? 0) + cost)
    }
  }

  const rows = [...byModel.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 4)
    .map(([name, cost]) => ({
      name: name.replace(/^claude-/, ''),
      cost,
      pct: totalCost > 0 ? (cost / totalCost) * 100 : 0,
    }))

  return { totalCost, rows }
})

function formatUSD(n: number): string {
  if (n >= 100) return `$${Math.round(n)}`
  if (n >= 1) return `$${n.toFixed(1)}`
  return `$${n.toFixed(2)}`
}
</script>

<template>
  <HomeCard icon="i-carbon-currency-dollar" title="费用估算" badge="本月">
    <div v-if="loading && !estimate" class="text-2xs text-muted-foreground py-2">统计中…</div>
    <template v-else-if="estimate">
      <div class="big-num">{{ formatUSD(estimate.totalCost) }}<small>估算</small></div>
      <div class="mt-2.5 flex flex-col gap-1.25">
        <div v-if="!estimate.rows.length" class="text-2xs text-muted-foreground">本月暂无用量</div>
        <div v-for="m in estimate.rows" :key="m.name" class="flex items-center gap-2 text-xs">
          <span class="w-20 text-muted-foreground font-mono text-2xs truncate" :title="m.name">{{ m.name }}</span>
          <span class="flex-1 h-1.25 rounded-sm bg-muted overflow-hidden">
            <span class="block h-full rounded-sm bg-accent" :style="{ width: `${m.pct}%` }" />
          </span>
          <span class="w-12 text-right tabular-nums text-muted-foreground text-2xs">{{ formatUSD(m.cost) }}</span>
        </div>
      </div>
      <div class="text-2xs text-muted-foreground mt-2">按官方定价粗估 · 不含折扣</div>
    </template>
    <div v-else class="text-2xs text-muted-foreground py-2">暂无数据</div>
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
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}
</style>
