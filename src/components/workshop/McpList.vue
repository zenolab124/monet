<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AssetItem from './AssetItem.vue'
import { mcpKey, type McpProbeState } from '@/composables/useWorkshop'
import type { WorkshopMcpServer } from '@/types'

const { t } = useI18n()

/**
 * MCP 服务器列表（FR-004/FR-005）：副行固定 `<transport> · <endpoint>`，
 * 三态展示（v2.9.0 FR-003）：enabled 绿 / disabled 降透明 / pending 琥珀。
 */

const props = defineProps<{
  items: WorkshopMcpServer[]
  probeStates: Map<string, McpProbeState>
}>()

function subline(s: WorkshopMcpServer): string {
  return `${s.transport} · ${s.endpoint}`
}

/** http/sse 行的探活状态；map 未就位时按「探测中…」展示 */
function stateOf(s: WorkshopMcpServer): McpProbeState {
  return props.probeStates.get(mcpKey(s)) ?? 'probing'
}
</script>

<template>
  <!-- 空态（MCP 无目录提示，口径为配置文件） -->
  <div v-if="items.length === 0" class="py-8 text-center">
    <p class="text-xs text-muted-foreground">{{ $t('workshop.noMcp') }}</p>
  </div>
  <div v-else class="mcp-grid">
    <AssetItem
      v-for="s in items"
      :key="mcpKey(s)"
      icon="i-carbon-plug"
      :name="s.name"
      :description="subline(s)"
      :source="s.source"
      :dimmed="s.status === 'disabled'"
    >
      <!-- 三态徽章 -->
      <span v-if="s.status === 'disabled'" class="mcp-badge text-muted-foreground border-border">{{ $t('workshop.mcpDisabled') }}</span>
      <span v-else-if="s.status === 'pending'" class="mcp-badge mcp-badge-pending" :title="$t('workshop.mcpPendingHint')">{{ $t('workshop.mcpPending') }}</span>
      <!-- 探活状态 -->
      <span v-if="s.transport !== 'http' && s.transport !== 'sse'" class="mcp-state text-muted-foreground">{{ $t('workshop.mcpTransportUnprobed', { transport: s.transport }) }}</span>
      <span v-else-if="stateOf(s) === 'online'" class="mcp-state text-primary">
        <span class="i-carbon-checkmark w-2.75 h-2.75" />{{ $t('workshop.mcpOnline') }}
      </span>
      <span v-else-if="stateOf(s) === 'offline'" class="mcp-state text-destructive">
        <span class="i-carbon-warning w-2.75 h-2.75" />{{ $t('workshop.mcpOffline') }}
      </span>
      <span v-else class="mcp-state text-muted-foreground">{{ $t('workshop.mcpProbing') }}</span>
    </AssetItem>
  </div>
</template>

<style scoped>
.mcp-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
@media (max-width: 768px) {
  .mcp-grid {
    grid-template-columns: 1fr;
  }
}
.mcp-state {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10.5px;
  flex-shrink: 0;
}
.mcp-badge {
  font-size: 10px;
  border: 1px solid var(--border);
  border-radius: 3px;
  padding: 0 5px;
  flex-shrink: 0;
}
.mcp-badge-pending {
  border-color: var(--amber, oklch(0.62 0.14 70));
  color: var(--amber, oklch(0.62 0.14 70));
  background: var(--amber-bg, oklch(0.92 0.04 70));
}
</style>
