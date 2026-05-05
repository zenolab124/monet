<script setup lang="ts">
import { onMounted, onUnmounted, computed } from 'vue'
import Toolbar from '@/components/Toolbar.vue'
import ProjectSidebar from '@/components/ProjectSidebar.vue'
import SessionList from '@/components/SessionList.vue'
import SplitView from '@/components/SplitView.vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { useUiState } from '@/composables/useUiState'
import { initPermissionListener } from '@/composables/usePermissionRequests'

const { loadProjects } = useProjects()
const { selectedSessionId, selectSession } = useSessions()
const { sidebarsCollapsed } = useUiState()

// 侧栏宽度（与原 w-56 / w-72 等价：224px / 288px）
const projectSidebarWidth = computed(() => (sidebarsCollapsed.value ? '0px' : '224px'))
const sessionListWidth = computed(() => (sidebarsCollapsed.value ? '0px' : '288px'))

function onKeydown(e: KeyboardEvent) {
  // Cmd+R: 刷新项目列表
  if ((e.metaKey || e.ctrlKey) && e.key === 'r') {
    e.preventDefault()
    loadProjects()
  }
  // Esc: 取消选择
  if (e.key === 'Escape') {
    selectSession(null)
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown)
  // FR-003 权限请求事件监听:整个 app 生命周期注册一次
  await initPermissionListener(() => selectedSessionId.value)
})
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="h-screen w-screen flex flex-col text-default app-bg" @contextmenu.prevent>
    <Toolbar />
    <div class="flex-1 flex min-h-0">
      <aside
        class="shrink-0 border-r border-divider overflow-hidden sidebar-collapsible"
        :class="{ 'border-r-0': sidebarsCollapsed }"
        :style="{ width: projectSidebarWidth }"
      >
        <ProjectSidebar />
      </aside>
      <section
        class="shrink-0 border-r border-divider overflow-hidden sidebar-collapsible"
        :class="{ 'border-r-0': sidebarsCollapsed }"
        :style="{ width: sessionListWidth }"
      >
        <SessionList />
      </section>
      <main class="flex-1 min-w-0">
        <SplitView />
      </main>
    </div>
  </div>
</template>

<style>
.app-bg {
  background-image: var(--bg-gradient);
}
.dark .app-bg {
  background-image: linear-gradient(to right, rgba(0,0,0,0.05) 0%, rgba(0,0,0,0.3) 15%, rgba(0,0,0,0.45) 35%, rgba(0,0,0,0.55) 100%);
  background-color: transparent;
}
/* 侧栏宽度过渡：贴合 macOS 抽屉手感 */
.sidebar-collapsible {
  transition: width 220ms cubic-bezier(0.32, 0.72, 0, 1);
}
</style>
