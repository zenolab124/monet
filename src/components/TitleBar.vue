<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useUiState, type AppSection } from '@/composables/useUiState'

const { t } = useI18n()
const { activeSection } = useUiState()

const sectionTitles = computed<Partial<Record<AppSection, string>>>(() => ({
  sessions: t('titlebar.archive'),
  workshop: t('titlebar.workshop'),
  automation: t('titlebar.automation'),
  home: t('titlebar.overview'),
  settings: t('titlebar.settings'),
}))
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
