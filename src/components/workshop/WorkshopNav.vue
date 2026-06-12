<script setup lang="ts">
import type { WorkshopCategory } from '@/composables/useWorkshop'

/** 工坊左侧子导航：四类资产切换 + 计数（加载中显示「…」） */

defineProps<{
  items: { key: WorkshopCategory; icon: string; label: string }[]
  counts: Record<WorkshopCategory, string>
  modelValue: WorkshopCategory
}>()

defineEmits<{ (e: 'update:modelValue', v: WorkshopCategory): void }>()
</script>

<template>
  <aside class="ws-nav shrink-0 border-r border-border">
    <div class="ws-pane-title text-muted-foreground">工坊</div>
    <div class="ws-list flex flex-col gap-0.5">
      <button
        v-for="item in items"
        :key="item.key"
        class="nav-item"
        :class="{ active: modelValue === item.key }"
        @click="$emit('update:modelValue', item.key)"
      >
        <span :class="item.icon" class="nav-icon" />
        <span class="nav-name">{{ item.label }}</span>
        <span class="nav-count text-muted-foreground">{{ counts[item.key] }}</span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
/* 内距逐像素对齐原型 layout-lab.html 工坊区：
   .forge-nav(12px 8px) + .pane-title(12px 16px 4px) + .ps-list(2px 8px 8px) */
.ws-nav {
  width: 190px;
  padding: 12px 8px;
}
.ws-pane-title {
  padding: 12px 16px 4px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
}
.ws-list {
  padding: 2px 8px 8px;
}
.nav-item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: var(--radius);
  cursor: pointer;
}
.nav-item:hover {
  background: var(--muted);
}
.nav-item.active {
  background: var(--card);
  box-shadow: var(--shadow-paper);
}
.nav-icon {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
  color: var(--muted-foreground);
}
.nav-item.active .nav-icon {
  color: var(--primary);
}
.nav-name {
  font-size: 12.5px;
  font-weight: 500;
  flex: 1;
  min-width: 0;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.nav-item.active .nav-name {
  color: var(--primary);
}
.nav-count {
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
</style>
