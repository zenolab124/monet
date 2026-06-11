<script setup lang="ts">
import { computed } from 'vue'
import Toolbar from '@/components/Toolbar.vue'
import ProjectSidebar from '@/components/ProjectSidebar.vue'
import SessionList from '@/components/SessionList.vue'
import SessionDetail from '@/components/SessionDetail.vue'
import { useUiState } from '@/composables/useUiState'

/**
 * 会话域：原顶层三栏整体降级为本组件挂入新壳（v2.0.0 FR-006）。
 * 交互不变；终态语义为档案馆（v2.1.0 只读化）。
 */

const { sidebarsCollapsed } = useUiState()

// 侧栏宽度（与原 w-56 / w-72 等价：224px / 288px）
const projectSidebarWidth = computed(() => (sidebarsCollapsed.value ? '0px' : '224px'))
const sessionListWidth = computed(() => (sidebarsCollapsed.value ? '0px' : '288px'))
</script>

<template>
  <div class="h-full flex flex-col">
    <Toolbar />
    <div class="flex-1 flex min-h-0">
      <aside
        class="shrink-0 border-r border-border overflow-hidden sidebar-collapsible"
        :class="{ 'border-r-0': sidebarsCollapsed }"
        :style="{ width: projectSidebarWidth }"
      >
        <ProjectSidebar />
      </aside>
      <section
        class="shrink-0 border-r border-border overflow-hidden sidebar-collapsible"
        :class="{ 'border-r-0': sidebarsCollapsed }"
        :style="{ width: sessionListWidth }"
      >
        <SessionList />
      </section>
      <!-- 档案馆详情:单面板只读(分屏已随只读化下线,多开用工作台多列) -->
      <main class="flex-1 min-w-0 p-2.5">
        <div class="h-full bg-card border border-border rounded shadow-paper overflow-hidden">
          <SessionDetail />
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
/* 侧栏宽度过渡：贴合 macOS 抽屉手感 */
.sidebar-collapsible {
  transition: width 220ms cubic-bezier(0.32, 0.72, 0, 1);
}
</style>
