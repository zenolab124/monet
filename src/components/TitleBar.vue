<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useUiState, type AppSection } from '@/composables/useUiState'
import { usePlatform } from '@/composables/usePlatform'
import TitleBarNotifications from '@/components/notifications/TitleBarNotifications.vue'

const { t } = useI18n()
const { isMac } = usePlatform()
const { activeSection, monitorRailCollapsed, toggleMonitorRail, peekMonitorRail, unpeekMonitorRail } = useUiState()

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
    <!-- macOS 红绿灯死区；其他平台有原生标题栏，无需让位 -->
    <div v-if="isMac" class="w-[78px] shrink-0" data-tauri-drag-region />
    <div v-else class="w-2 shrink-0" data-tauri-drag-region />

    <button
      v-if="activeSection === 'workbench'"
      class="w-5 h-5 grid place-items-center rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors shrink-0 mr-1"
      :title="monitorRailCollapsed ? $t('workbench.rail.show') : $t('workbench.rail.hide')"
      @click="toggleMonitorRail"
      @mouseenter="peekMonitorRail"
      @mouseleave="unpeekMonitorRail"
    >
      <span
        class="w-3.5 h-3.5 block"
        :class="monitorRailCollapsed ? 'i-carbon-side-panel-open' : 'i-carbon-side-panel-close'"
      />
    </button>

    <div class="flex items-center min-w-0 h-full">
      <span v-if="!$slots.leading && sectionTitles[activeSection]" class="text-xs font-medium text-foreground">
        {{ sectionTitles[activeSection] }}
      </span>
      <slot name="leading" />
    </div>

    <TitleBarNotifications />

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
