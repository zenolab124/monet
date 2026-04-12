<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import Toolbar from '@/components/Toolbar.vue'
import ProjectSidebar from '@/components/ProjectSidebar.vue'
import SessionList from '@/components/SessionList.vue'
import SplitView from '@/components/SplitView.vue'
import { useSplitLayout } from '@/composables/useSplitLayout'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'

const { root } = useSplitLayout()
const { loadProjects } = useProjects()
const { selectSession } = useSessions()

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

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="h-screen w-screen flex flex-col text-default app-bg" @contextmenu.prevent>
    <Toolbar />
    <div class="flex-1 flex min-h-0">
      <aside class="w-56 shrink-0 border-r border-divider">
        <ProjectSidebar />
      </aside>
      <section class="w-72 shrink-0 border-r border-divider">
        <SessionList />
      </section>
      <main class="flex-1 min-w-0">
        <SplitView :node="root" />
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
</style>
