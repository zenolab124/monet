<script setup lang="ts">
/** 工坊 fitem 行：只读陈列，无点击交互（详情查看在 Out of Scope） */

withDefaults(defineProps<{
  icon: string
  name: string
  /** 版本徽章（仅 Skills 且有值时显示，渲染为 v 前缀） */
  version?: string | null
  description: string
  /** 描述悬停 title，缺省用 description 本身 */
  descTitle?: string
  source: string
  /** 整行降不透明度（MCP 已禁用行） */
  dimmed?: boolean
}>(), { version: null, descTitle: undefined, dimmed: false })
</script>

<template>
  <div class="fitem" :class="{ 'opacity-60': dimmed }">
    <span :class="icon" class="w-3.75 h-3.75 shrink-0 text-primary" />
    <div class="flex-1 min-w-0">
      <div class="fi-name">
        <span class="truncate">{{ name }}</span>
        <span v-if="version" class="fi-ver text-muted-foreground">v{{ version }}</span>
      </div>
      <div class="fi-desc text-muted-foreground" :title="descTitle ?? description">{{ description }}</div>
    </div>
    <!-- 徽章插槽（MCP 探活/禁用状态） -->
    <slot />
    <span class="fi-src text-muted-foreground border-border">{{ source }}</span>
  </div>
</template>

<style scoped>
.fitem {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  padding: 10px 14px;
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  overflow: hidden;
}
.fi-name {
  font-size: 12.5px;
  font-weight: 600;
  display: flex;
  align-items: baseline;
  gap: 6px;
  min-width: 0;
}
.fi-ver {
  font-size: 10.5px;
  font-family: var(--font-mono);
  font-weight: 400;
}
.fi-desc {
  font-size: 11.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.fi-src {
  font-size: 10px;
  border: 1px solid var(--border);
  border-radius: 3px;
  padding: 0 5px;
  flex-shrink: 0;
}
</style>
