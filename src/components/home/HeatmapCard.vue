<script setup lang="ts">
import { computed } from 'vue'
import type { UsageStats } from '../../types'
import { formatTokens } from '../../types'
import HomeCard from './HomeCard.vue'

/**
 * 活跃热力卡（v2.2.0 FR-003）：近 16 周按天 token 量五档格子。
 * 列 = 周（最后一列本周）、行 = 周一~周日；本周未到的天渲染隐形占位保持网格对齐。
 * 档位：0 无数据；非零日按 P25/P50/P75 分位切 1~4；非零日 < 4 天全记 2。
 */
const props = defineProps<{
  usage: UsageStats | null
  loading: boolean
  error: string | null
}>()

const emit = defineEmits<{ retry: []; 'select-date': [date: string] }>()

const WEEKS = 16

interface Cell {
  date: string
  total: number
  level: number
  future: boolean
}

/** 本地日期 → "YYYY-MM-DD"（不能用 toISOString，UTC 偏移会窜天） */
function fmtDate(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${m}-${day}`
}

const rows = computed<Cell[][]>(() => {
  const byDate = new Map((props.usage?.daily ?? []).map((d) => [d.date, d.total]))

  const today = new Date()
  today.setHours(0, 0, 0, 0)
  // 窗口起点 = 本周一往前 15 周（与 Rust usage_stats 同口径）
  const start = new Date(today)
  start.setDate(start.getDate() - ((today.getDay() + 6) % 7) - 15 * 7)

  const nonzero = [...byDate.values()].filter((v) => v > 0).sort((a, b) => a - b)
  // nearest-rank 取分位：floor 会在 n=4 时让 P75 落到最大值、level 4 永不可达
  const quantile = (p: number) => nonzero[Math.max(0, Math.ceil(p * nonzero.length) - 1)]
  const levelOf = (v: number) => {
    if (v === 0) return 0
    if (nonzero.length < 4) return 2
    if (v <= quantile(0.25)) return 1
    if (v <= quantile(0.5)) return 2
    if (v <= quantile(0.75)) return 3
    return 4
  }

  return Array.from({ length: 7 }, (_, day) =>
    Array.from({ length: WEEKS }, (_, week) => {
      const d = new Date(start)
      d.setDate(d.getDate() + week * 7 + day)
      const date = fmtDate(d)
      const total = byDate.get(date) ?? 0
      return { date, total, level: levelOf(total), future: d > today }
    }),
  )
})
</script>

<template>
  <HomeCard icon="i-carbon-grid" title="活跃热力" badge="近 16 周" wide>
    <template v-if="error">
      <div class="py-3 text-xs text-muted-foreground">加载失败</div>
      <button class="retry-btn" @click="emit('retry')">重试</button>
    </template>
    <template v-else>
      <div class="flex flex-col gap-0.75 overflow-x-auto">
        <div v-for="(row, di) in rows" :key="di" class="flex gap-0.75">
          <span
            v-for="cell in row"
            :key="cell.date"
            class="hm-cell shrink-0"
            :class="[
              cell.future ? 'future' : cell.level ? `l${cell.level}` : '',
              !cell.future && cell.total > 0 ? 'clickable' : '',
            ]"
            :title="cell.future ? undefined : `${cell.date} · ${formatTokens(cell.total)} tokens`"
            @click="!cell.future && cell.total > 0 && emit('select-date', cell.date)"
          />
        </div>
      </div>
      <div class="flex items-center justify-end gap-1 mt-2 text-2xs text-muted-foreground">
        少
        <span class="hm-cell sm" /><span class="hm-cell sm l1" /><span class="hm-cell sm l2" /><span class="hm-cell sm l3" /><span class="hm-cell sm l4" />
        多
      </div>
    </template>
  </HomeCard>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.4;
}

.hm-cell {
  width: 12px;
  height: 12px;
  border-radius: 2px;
  background: var(--muted);
}
.hm-cell.sm {
  width: 9px;
  height: 9px;
}
.hm-cell.future {
  visibility: hidden;
}
.hm-cell.clickable {
  cursor: pointer;
}
.hm-cell.clickable:hover {
  outline: 1.5px solid var(--foreground);
  outline-offset: -1px;
}
.hm-cell.l1 {
  background: color-mix(in oklch, var(--primary) 25%, var(--card));
}
.hm-cell.l2 {
  background: color-mix(in oklch, var(--primary) 45%, var(--card));
}
.hm-cell.l3 {
  background: color-mix(in oklch, var(--primary) 70%, var(--card));
}
.hm-cell.l4 {
  background: var(--primary);
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
