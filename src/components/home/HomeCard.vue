<script setup lang="ts">
/** 首页卡片外壳：头部（图标/标题/badge）+ 内容 slot；wide 跨双列 */
defineProps<{
  icon: string
  title: string
  badge: string
  wide?: boolean
}>()
</script>

<template>
  <div class="home-card" :class="{ wide }">
    <div class="flex items-center gap-1.5 mb-2.5">
      <span :class="icon" class="w-3.75 h-3.75 text-primary" />
      <span class="text-sm font-semibold">{{ title }}</span>
      <span class="hc-badge">{{ badge }}</span>
    </div>
    <slot />
  </div>
</template>

<style scoped>
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

@media (prefers-reduced-motion: reduce) {
  .home-card {
    transition: none !important;
  }
}
</style>
