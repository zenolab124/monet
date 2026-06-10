<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import ActivityBar from '@/components/ActivityBar.vue'
import SessionsView from '@/views/SessionsView.vue'
import HomeView from '@/views/HomeView.vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { useUiState } from '@/composables/useUiState'
import { initPermissionListener } from '@/composables/usePermissionRequests'

const { loadProjects } = useProjects()
const { selectedSessionId, selectSession } = useSessions()
const { activeSection } = useUiState()

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
  // 权限请求事件监听:整个 app 生命周期注册一次
  await initPermissionListener(() => selectedSessionId.value)
})
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <!-- 新壳（v2.0.0 FR-006）：ActivityBar + 区域 v-show 切换（DOM 常驻，流式/滚动/监听零丢失） -->
  <div class="h-screen w-screen flex bg-background text-foreground" @contextmenu.prevent>
    <ActivityBar />
    <SessionsView v-show="activeSection === 'sessions'" class="flex-1 min-w-0" />
    <HomeView v-show="activeSection === 'home'" class="flex-1 min-w-0" />
  </div>
</template>
