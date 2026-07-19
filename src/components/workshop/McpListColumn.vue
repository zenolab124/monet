<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { mcpKey, type McpProbeState } from '@/composables/useWorkshop'
import type { WorkshopMcpServer } from '@/types'

/**
 * MCP 列表列（v2.9.0 FR-002/FR-003）：三栏骨架中间列的 MCP 服务器行。
 * 三态徽章：enabled 绿「启用」、disabled「已禁用」+整行降透明、pending 琥珀「待批准」。
 */

const { t } = useI18n()

const props = defineProps<{
  items: WorkshopMcpServer[]
  probeStates: Map<string, McpProbeState>
  selectedPath: string | null
  narrow?: boolean
}>()

const emit = defineEmits<{
  (e: 'select', path: string): void
}>()

/** 用 path + name 做唯一标识 */
function itemKey(s: WorkshopMcpServer): string {
  return mcpKey(s)
}

function subline(s: WorkshopMcpServer): string {
  return `${s.transport} · ${s.endpoint}`
}

function stateOf(s: WorkshopMcpServer): McpProbeState {
  return props.probeStates.get(mcpKey(s)) ?? 'probing'
}
</script>

<template>
  <!-- 空态 -->
  <div v-if="items.length === 0" class="list-empty">
    <p class="text-xs text-muted-foreground">{{ t('workshop.noMcp') }}</p>
  </div>
  <div v-else class="list-rows">
    <div
      v-for="s in items"
      :key="itemKey(s)"
      class="fitem"
      :class="{
        selected: selectedPath === s.path,
        'disabled-row': s.status === 'disabled'
      }"
      @click="emit('select', s.path)"
    >
      <span class="i-carbon-plug fi-icon" />
      <div class="fi-body">
        <div class="fi-name">{{ s.name }}</div>
        <div v-if="!narrow" class="fi-sub">{{ subline(s) }}</div>
      </div>
      <!-- 探活指示器 -->
      <template v-if="!narrow">
        <span v-if="s.transport === 'http' || s.transport === 'sse'" class="probe-dot" :class="{
          'probe-online': stateOf(s) === 'online',
          'probe-offline': stateOf(s) === 'offline',
        }" />
      </template>
      <!-- 三态徽章 -->
      <span
        class="status-badge"
        :class="{
          'badge-enabled': s.status === 'enabled',
          'badge-disabled': s.status === 'disabled',
          'badge-pending': s.status === 'pending',
        }"
        :title="s.status === 'pending' ? t('workshop.mcpPendingHint') : undefined"
      >
        {{ s.status === 'enabled' ? t('workshop.mcpEnabled') : s.status === 'disabled' ? t('workshop.mcpDisabled') : t('workshop.mcpPending') }}
      </span>
      <span v-if="!narrow" class="fi-src">{{ s.source }}</span>
    </div>
  </div>
</template>

<style scoped>
.list-empty {
  padding: 32px 14px;
  text-align: center;
}
.list-rows {
  display: flex;
  flex-direction: column;
}
.fitem {
  background: var(--card);
  border-bottom: 1px solid var(--border);
  padding: 10px 14px;
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
}
.fitem:hover {
  background: var(--muted);
}
.fitem.selected {
  background: var(--secondary);
}
.fitem.disabled-row {
  opacity: 0.5;
}
.fi-icon {
  width: 14px;
  height: 14px;
  color: var(--primary);
  flex-shrink: 0;
}
.fi-body {
  flex: 1;
  min-width: 0;
}
.fi-name {
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.fi-sub {
  font-size: 10px;
  color: var(--muted-foreground);
  font-family: var(--font-mono);
  margin-top: 1px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.probe-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--muted-foreground);
}
.probe-online {
  background: var(--primary);
}
.probe-offline {
  background: var(--destructive);
}
.status-badge {
  font-size: 9.5px;
  font-weight: 500;
  border-radius: 3px;
  padding: 1px 5px;
  white-space: nowrap;
  flex-shrink: 0;
}
.badge-enabled {
  background: oklch(0.92 0.03 145);
  color: var(--primary);
}
.badge-disabled {
  background: var(--muted);
  color: var(--muted-foreground);
}
.badge-pending {
  background: var(--amber-bg, oklch(0.92 0.04 70));
  color: var(--amber, oklch(0.62 0.14 70));
}
.fi-src {
  font-size: 9.5px;
  font-weight: 500;
  border-radius: 3px;
  padding: 1px 5px;
  white-space: nowrap;
  flex-shrink: 0;
  border: 1px solid var(--border);
  color: var(--muted-foreground);
}
</style>
