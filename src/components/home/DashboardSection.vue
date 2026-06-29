<script setup lang="ts">
defineProps<{
  icon: string
  title: string
  badge?: string
}>()
</script>

<template>
  <section class="dashboard-section">
    <header class="section-header">
      <span class="w-3.5 h-3.5" :class="icon" />
      <span class="section-title">{{ title }}</span>
      <span v-if="badge" class="section-badge">{{ badge }}</span>
    </header>
    <div class="section-body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.dashboard-section {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper, 0 1px 3px rgba(0,0,0,.08));
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.section-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 12px;
  color: var(--muted-foreground);
}
.section-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--foreground);
}
.section-badge {
  font-size: 10px;
  color: var(--muted-foreground);
  margin-left: auto;
}
.section-body {
  flex: 1;
  min-height: 0;
}
</style>
