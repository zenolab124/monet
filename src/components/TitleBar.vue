<script setup lang="ts">
import { useUiState, type AppSection } from '@/composables/useUiState'

const { activeSection } = useUiState()

const sectionTitles: Partial<Record<AppSection, string>> = {
  sessions: '档案',
  workshop: '工坊',
  automation: '自动化',
  home: '总览',
  settings: '设置',
}
</script>

<template>
  <header class="titlebar">
    <div class="w-[78px] shrink-0" data-tauri-drag-region />

    <div class="flex items-center min-w-0 h-full">
      <span v-if="!$slots.leading && sectionTitles[activeSection]" class="text-xs font-medium text-foreground">
        {{ sectionTitles[activeSection] }}
      </span>
      <slot name="leading" />
    </div>

    <div class="flex-1 min-w-0 h-full" data-tauri-drag-region />

    <div class="flex items-center gap-1.5 pr-3 shrink-0">
      <slot name="trailing" />
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  height: 32px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  border-bottom: 1px solid var(--border);
  background: var(--secondary);
}
</style>
