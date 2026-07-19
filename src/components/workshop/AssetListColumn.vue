<script setup lang="ts">
/**
 * 列表列（v2.9.0 FR-002）：三栏骨架的中间列。
 * 紧凑行式列表，单行描述截断，行可点击选中（高亮态）。
 * 响应式：<900px 时收窄为窄列（只显名称）。
 */

interface AssetRowLike {
  name: string
  description: string
  version?: string | null
  source: string
  path: string
}

const props = defineProps<{
  items: AssetRowLike[]
  icon: string
  selectedPath: string | null
  emptyTitle: string
  emptyHint: string
  narrow?: boolean
}>()

const emit = defineEmits<{
  (e: 'select', path: string): void
}>()
</script>

<template>
  <!-- 空态 -->
  <div v-if="items.length === 0" class="list-empty">
    <p class="text-xs text-muted-foreground">{{ emptyTitle }}</p>
    <p class="text-xs text-muted-foreground mt-1">{{ emptyHint }}</p>
  </div>
  <div v-else class="list-rows">
    <div
      v-for="item in items"
      :key="`${item.path}|${item.name}`"
      class="fitem"
      :class="{ selected: selectedPath === item.path }"
      @click="emit('select', item.path)"
    >
      <span :class="icon" class="fi-icon" />
      <div class="fi-body">
        <div class="fi-name">
          <span class="fi-name-text">{{ item.name }}</span>
          <span v-if="item.version && !narrow" class="fi-ver">v{{ item.version }}</span>
        </div>
        <div v-if="!narrow" class="fi-desc">{{ item.description }}</div>
      </div>
      <span v-if="!narrow" class="fi-src">{{ item.source }}</span>
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
  display: flex;
  align-items: baseline;
  gap: 5px;
  white-space: nowrap;
}
.fi-name-text {
  overflow: hidden;
  text-overflow: ellipsis;
}
.fi-ver {
  font-size: 10px;
  color: var(--muted-foreground);
  font-family: var(--font-mono);
  font-weight: 400;
}
.fi-desc {
  font-size: 11px;
  color: var(--muted-foreground);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 1px;
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
