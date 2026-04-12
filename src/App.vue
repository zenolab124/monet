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
  <div class="h-screen w-screen flex flex-col bg-bg text-default">
    <Toolbar />
    <div class="flex-1 flex min-h-0">
      <aside class="w-56 shrink-0 border-r border-divider bg-nav">
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
