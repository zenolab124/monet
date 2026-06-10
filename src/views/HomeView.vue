<script setup lang="ts">
/**
 * 首页占位版式（v2.0.0 FR-007）：纯静态三卡，零数据零 IPC。
 * 版式对齐 docs/prototypes/shell.html 首页区；数据实现归后续迭代。
 */

/** 占位数字直接采用原型示例值 */
const tokenModels = [
  { name: 'fable-5', pct: 65, amount: '8.1M' },
  { name: 'opus-4.8', pct: 26, amount: '3.2M' },
  { name: 'haiku-4.5', pct: 9, amount: '1.1M' },
]

const diagRows = [
  { ok: true, label: '已支持记录类型', count: '31 种' },
  { ok: false, label: '未识别记录类型（挂起观察）', count: '4 种' },
  { ok: false, label: 'Generic 兜底工具', count: '6 个' },
]

/** 热力占位：固定种子伪随机（同原型算法），每次打开必然一致 */
const WEEKS = 52
const heatLevels: number[] = (() => {
  let s = 7
  const rand = () => (s = (s * 16807) % 2147483647) / 2147483647
  const levels: number[] = []
  for (let i = 0; i < WEEKS * 7; i++) {
    const v = rand()
    levels.push(v < 0.3 ? 0 : v < 0.55 ? 1 : v < 0.75 ? 2 : v < 0.9 ? 3 : 4)
  }
  return levels
})()

/** 行优先排布：7 行（周内日）× 52 列（周） */
const heatRows: number[][] = Array.from({ length: 7 }, (_, day) =>
  Array.from({ length: WEEKS }, (_, week) => heatLevels[week * 7 + day]),
)
</script>

<template>
  <main class="h-full overflow-y-auto px-8 py-6.5" data-tauri-drag-region>
    <div class="max-w-220 mx-auto">
      <div class="flex items-baseline gap-3 mb-4.5">
        <h1 class="text-lg font-semibold">总览</h1>
        <span class="text-xs text-muted-foreground">2026 年 6 月 10 日 · 本月数据</span>
      </div>

      <div class="card-grid">
        <!-- Token 消耗占位 -->
        <div class="home-card">
          <div class="flex items-center gap-1.5 mb-2.5">
            <span class="i-carbon-meter w-3.75 h-3.75 text-primary" />
            <span class="text-sm font-semibold">Token 消耗</span>
            <span class="hc-badge">本月</span>
          </div>
          <div class="big-num">12.4M<small>tokens</small></div>
          <div class="mt-2.5 flex flex-col gap-1.25">
            <div v-for="m in tokenModels" :key="m.name" class="flex items-center gap-2 text-xs">
              <span class="w-20 text-muted-foreground font-mono text-2xs">{{ m.name }}</span>
              <span class="flex-1 h-1.25 rounded-sm bg-muted overflow-hidden">
                <span class="block h-full rounded-sm bg-primary" :style="{ width: `${m.pct}%` }" />
              </span>
              <span class="w-12 text-right tabular-nums text-muted-foreground text-2xs">{{ m.amount }}</span>
            </div>
          </div>
        </div>

        <!-- 兼容性诊断占位 -->
        <div class="home-card">
          <div class="flex items-center gap-1.5 mb-2.5">
            <span class="i-carbon-activity w-3.75 h-3.75 text-primary" />
            <span class="text-sm font-semibold">兼容性诊断</span>
            <span class="hc-badge">schema-probe</span>
          </div>
          <div
            v-for="row in diagRows"
            :key="row.label"
            class="diag-row flex items-center gap-2 text-xs py-1.25"
          >
            <span
              :class="row.ok ? 'i-carbon-checkmark text-primary' : 'i-carbon-warning-alt text-accent'"
              class="w-3.25 h-3.25"
            />
            <span>{{ row.label }}</span>
            <span class="ml-auto text-2xs text-muted-foreground tabular-nums">{{ row.count }}</span>
          </div>
          <div class="mt-2 text-2xs text-muted-foreground">上次扫描 2026-06-10 · 2781 个会话文件</div>
        </div>

        <!-- 活跃热力占位（全宽） -->
        <div class="home-card wide">
          <div class="flex items-center gap-1.5 mb-2.5">
            <span class="i-carbon-grid w-3.75 h-3.75 text-primary" />
            <span class="text-sm font-semibold">活跃热力</span>
            <span class="hc-badge">近 52 周</span>
          </div>
          <div class="flex flex-col gap-0.75 overflow-x-auto">
            <div v-for="(row, di) in heatRows" :key="di" class="flex gap-0.75">
              <span
                v-for="(lv, wi) in row"
                :key="wi"
                class="hm-cell shrink-0"
                :class="lv ? `l${lv}` : ''"
              />
            </div>
          </div>
          <div class="flex items-center justify-end gap-1 mt-2 text-2xs text-muted-foreground">
            少
            <span class="hm-cell sm" /><span class="hm-cell sm l1" /><span class="hm-cell sm l2" /><span class="hm-cell sm l3" /><span class="hm-cell sm l4" />
            多
          </div>
        </div>
      </div>
    </div>
  </main>
</template>

<style scoped>
.card-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
/* 窄窗降单列（FR-007 验收口径 <900px） */
@media (max-width: 900px) {
  .card-grid {
    grid-template-columns: 1fr;
  }
}

.home-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  padding: 14px 16px;
  transition: box-shadow 0.18s, transform 0.18s;
}
/* hover「拈起」 */
.home-card:hover {
  box-shadow: var(--shadow-paper-lifted);
  transform: translateY(-1px);
}
.home-card.wide {
  grid-column: 1 / -1;
}

.hc-badge {
  margin-left: auto;
  font-size: 10px;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--muted-foreground);
}

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

.diag-row {
  border-bottom: 1px solid var(--border);
}
.diag-row:last-of-type {
  border-bottom: none;
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

@media (prefers-reduced-motion: reduce) {
  .home-card {
    transition: none !important;
  }
}
</style>
