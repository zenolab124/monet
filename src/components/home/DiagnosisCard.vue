<script setup lang="ts">
import { computed } from 'vue'
import type { SchemaDiagnosis } from '../../types'
import HomeCard from './HomeCard.vue'

/**
 * 兼容性诊断卡（v2.2.0 FR-005）。指标口径：
 * 已支持 = 数据中实际出现过的已支持记录类型数；未识别 = unknown 键数；
 * Generic 兜底 = 未决策的 generic_undeclared 键数（严格口径，genericOk 不计）。
 */
const props = defineProps<{
  diag: SchemaDiagnosis | null
  loading: boolean
  error: string | null
  scannedAt: Date | null
}>()

const emit = defineEmits<{ retry: [] }>()

const rows = computed(() => {
  if (!props.diag) return []
  const supported = Object.keys(props.diag.record_types.supported).length
  const unknown = Object.keys(props.diag.record_types.unknown).length
  const generic = Object.keys(props.diag.tools.generic_undeclared).length
  return [
    { ok: true, label: '已支持记录类型', count: `${supported} 种` },
    { ok: unknown === 0, label: '未识别记录类型（挂起观察）', count: `${unknown} 种` },
    { ok: generic === 0, label: 'Generic 兜底工具', count: `${generic} 个` },
  ]
})

/** 加载态三行占位（计数位「…」） */
const placeholderRows = [
  { ok: true, label: '已支持记录类型', count: '…' },
  { ok: true, label: '未识别记录类型（挂起观察）', count: '…' },
  { ok: true, label: 'Generic 兜底工具', count: '…' },
]

const footer = computed(() => {
  if (props.loading) return '扫描中…'
  if (!props.diag || !props.scannedAt) return ''
  const d = props.scannedAt
  const pad = (n: number) => String(n).padStart(2, '0')
  const stamp = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
  return `上次扫描 ${stamp} · ${props.diag.scanned_files} 个会话文件`
})
</script>

<template>
  <HomeCard icon="i-carbon-activity" title="兼容性诊断" badge="schema-probe">
    <template v-if="error">
      <div class="py-3 text-xs text-muted-foreground">加载失败</div>
      <button class="retry-btn" @click="emit('retry')">重试</button>
    </template>
    <template v-else>
      <div
        v-for="row in (loading ? placeholderRows : rows)"
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
      <div class="mt-2 text-2xs text-muted-foreground">{{ footer }}</div>
    </template>
  </HomeCard>
</template>

<style scoped>
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
