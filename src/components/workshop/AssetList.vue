<script setup lang="ts">
import AssetItem from './AssetItem.vue'

/**
 * 通用资产列表（Skills / Commands / Subagents，FR-003）：
 * 排序由 Rust 端完成（先全局后项目级，组内名称字典序），前端按序直渲。
 */

/** 三类资产的结构公共面（commands 的 argumentHint 不展示，PRD FR-003 口径） */
interface AssetRowLike {
  name: string
  description: string
  version?: string | null
  source: string
  path: string
}

defineProps<{
  items: AssetRowLike[]
  icon: string
  emptyTitle: string
  emptyHint: string
}>()
</script>

<template>
  <!-- 空态：未发现任何 <类别名> + 全局目录路径提示 -->
  <div v-if="items.length === 0" class="py-8 text-center">
    <p class="text-xs text-muted-foreground">{{ emptyTitle }}</p>
    <p class="text-xs text-muted-foreground mt-1">{{ emptyHint }}</p>
  </div>
  <div v-else>
    <AssetItem
      v-for="item in items"
      :key="`${item.path}|${item.name}`"
      :icon="icon"
      :name="item.name"
      :version="item.version"
      :description="item.description"
      :source="item.source"
    />
  </div>
</template>
