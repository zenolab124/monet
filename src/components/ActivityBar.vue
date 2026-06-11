<script setup lang="ts">
import { computed } from 'vue'
import { useUiState, type AppSection } from '@/composables/useUiState'
import { useTheme } from '@/composables/useTheme'
import { useNotifications } from '@/composables/useNotifications'

const { activeSection, switchSection } = useUiState()
const { mode, cycleTheme, themeLabel, themeIcon } = useTheme()
const { badgeCount } = useNotifications()

/** 终态七域全摆；v2.1.0 点亮工作台,「会话」更名档案语义,其余灰置 */
interface DomainItem {
  key: string
  icon: string
  label: string
  section?: AppSection
}

const topDomains: DomainItem[] = [
  { key: 'workbench', icon: 'i-carbon-workspace', label: '工作台', section: 'workbench' },
  { key: 'sessions', icon: 'i-carbon-chat', label: '档案', section: 'sessions' },
  { key: 'search', icon: 'i-carbon-search', label: '搜索' },
  { key: 'workshop', icon: 'i-carbon-tools', label: '工坊' },
  { key: 'automation', icon: 'i-carbon-bot', label: '自动化' },
  { key: 'home', icon: 'i-carbon-home', label: '首页', section: 'home' },
]

function onItemClick(item: DomainItem) {
  if (item.section) switchSection(item.section)
}

/** 工作台角标(FR-007):未处理持久型事件数(等权限+出错),0 隐藏,上限 9+ */
const badgeText = computed(() => {
  if (badgeCount.value <= 0) return null
  return badgeCount.value > 9 ? '9+' : String(badgeCount.value)
})
</script>

<template>
  <nav
    class="w-12 shrink-0 flex flex-col items-center gap-1 pb-2.5 bg-secondary border-r border-border"
    data-tauri-drag-region
  >
    <!-- macOS 红绿灯区（Overlay 标题栏），同时是窗口拖拽区 -->
    <div class="h-9 shrink-0 w-full" data-tauri-drag-region />

    <button
      v-for="item in topDomains"
      :key="item.key"
      class="ab-item"
      :class="item.section
        ? { active: activeSection === item.section }
        : 'ab-disabled'"
      :title="item.section ? item.label : `${item.label}（即将推出）`"
      :disabled="!item.section"
      @click="onItemClick(item)"
    >
      <span :class="item.icon" class="w-4.5 h-4.5 block" />
      <span
        v-if="item.key === 'workbench' && badgeText"
        class="ab-badge"
      >{{ badgeText }}</span>
    </button>

    <div class="flex-1" data-tauri-drag-region />

    <!-- 外观切换（自 Toolbar 移入，全域可达） -->
    <button class="ab-item" :title="themeLabel[mode]" @click="cycleTheme">
      <span :class="themeIcon[mode]" class="w-4.5 h-4.5 block" />
    </button>

    <!-- 设置（v2.1.0+ 点亮） -->
    <button class="ab-item ab-disabled" title="设置（即将推出）" disabled>
      <span class="i-carbon-settings w-4.5 h-4.5 block" />
    </button>
  </nav>
</template>

<style scoped>
.ab-item {
  width: 34px;
  height: 34px;
  border-radius: var(--radius);
  display: grid;
  place-items: center;
  color: var(--muted-foreground);
  position: relative;
  transition: color 0.15s, background-color 0.15s;
}
.ab-item:not(.ab-disabled):hover {
  color: var(--foreground);
  background: var(--muted);
}
/* 选中态 =「纸片拈起」：card 底 + paper 阴影 + 左侧 2px primary 指示条 */
.ab-item.active {
  color: var(--primary);
  background: var(--card);
  box-shadow: var(--shadow-paper);
}
.ab-item.active::before {
  content: '';
  position: absolute;
  left: -7px;
  top: 8px;
  bottom: 8px;
  width: 2px;
  border-radius: 1px;
  background: var(--primary);
}
/* 灰置域：无 hover 反馈 */
.ab-disabled {
  opacity: 0.35;
  cursor: default;
}
/* 工作台角标:accent 单一视觉信号,不随事件类型变色 */
.ab-badge {
  position: absolute;
  top: 2px;
  right: 2px;
  min-width: 13px;
  height: 13px;
  border-radius: 3px;
  background: var(--accent);
  color: var(--accent-foreground);
  font-size: 9px;
  line-height: 13px;
  text-align: center;
  padding: 0 2px;
}
</style>
