<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SchemaDiagnosis } from '../../types'
import DashboardSection from './DashboardSection.vue'

const props = defineProps<{
  diag: SchemaDiagnosis | null
  loading: boolean
  error: string | null
  scannedAt: Date | null
}>()

const { t } = useI18n()

const emit = defineEmits<{ retry: [] }>()

const rows = computed(() => {
  if (!props.diag) return []
  const supported = Object.keys(props.diag.record_types.supported).length
  const unknown = Object.keys(props.diag.record_types.unknown).length
  const generic = Object.keys(props.diag.tools.generic_undeclared).length
  return [
    { ok: true, label: t('home.diagnosis.supportedTypes'), count: `${supported} 种` },
    { ok: unknown === 0, label: t('home.diagnosis.unknownTypes'), count: `${unknown} 种` },
    { ok: generic === 0, label: t('home.diagnosis.genericTools'), count: `${generic} 个` },
  ]
})

const placeholderRows = computed(() => [
  { ok: true, label: t('home.diagnosis.supportedTypes'), count: '…' },
  { ok: true, label: t('home.diagnosis.unknownTypes'), count: '…' },
  { ok: true, label: t('home.diagnosis.genericTools'), count: '…' },
])

const footer = computed(() => {
  if (props.loading) return t('home.diagnosis.scanning')
  if (!props.diag || !props.scannedAt) return ''
  const d = props.scannedAt
  const pad = (n: number) => String(n).padStart(2, '0')
  const stamp = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
  return t('home.diagnosis.footer', { stamp, count: props.diag.scanned_files })
})
</script>

<template>
  <DashboardSection icon="i-carbon-activity" :title="$t('home.diagnosis.title')" badge="schema-probe">
    <template v-if="error">
      <div class="py-3 text-xs text-muted-foreground">{{ $t('common.loadFailed') }}</div>
      <button class="retry-btn" @click="emit('retry')">{{ $t('common.retry') }}</button>
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
  </DashboardSection>
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
