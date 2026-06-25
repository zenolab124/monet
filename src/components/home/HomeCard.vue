<script setup lang="ts">
defineProps<{
  icon: string
  title: string
  badge: string
  wide?: boolean
}>()
</script>

<template>
  <div class="home-card">
    <div class="hc-header">
      <span :class="icon" class="w-3.75 h-3.75 text-primary" />
      <span class="text-sm font-semibold">{{ title }}</span>
      <span class="hc-badge">{{ badge }}</span>
    </div>
    <div class="hc-body">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.home-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  padding: 14px 16px;
  height: 100%;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.hc-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 10px;
  flex-shrink: 0;
}

.hc-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.hc-badge {
  margin-left: auto;
  font-size: 10px;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--muted-foreground);
  flex-shrink: 0;
}
</style>
