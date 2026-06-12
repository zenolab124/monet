<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import ActivityBar from '@/components/ActivityBar.vue'
import SessionsView from '@/views/SessionsView.vue'
import HomeView from '@/views/HomeView.vue'
import WorkbenchView from '@/views/WorkbenchView.vue'
import WorkshopView from '@/views/WorkshopView.vue'
import AutomationView from '@/views/AutomationView.vue'
import SettingsView from '@/views/SettingsView.vue'
import ToastStack from '@/components/notifications/ToastStack.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { useUiState } from '@/composables/useUiState'
import { initPermissionListener } from '@/composables/usePermissionRequests'
import { initStreamListeners } from '@/composables/useStreaming'
import { initNotificationLayer, useNotifications } from '@/composables/useNotifications'
import { stateWasReset, useWorkbench } from '@/composables/useWorkbench'

const { projects, loadProjects } = useProjects()
const { selectSession } = useSessions()
const { activeSection } = useUiState()
const { pruneDrafts } = useWorkbench()

// 草稿会话收割:projects 每次刷新后,把已落盘(或已被关闭弃用)的草稿清掉
watch(projects, (list) => {
  const ids = new Set(list.flatMap(p => p.sessions.map(s => s.id)))
  pruneDrafts(sid => ids.has(sid))
})

function onKeydown(e: KeyboardEvent) {
  // Cmd+R: 刷新项目列表
  if ((e.metaKey || e.ctrlKey) && e.key === 'r') {
    e.preventDefault()
    loadProjects()
  }
  // Esc: 取消档案馆选择
  if (e.key === 'Escape' && activeSection.value === 'sessions') {
    selectSession(null)
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown)
  // 全局事件监听:整个 app 生命周期各注册一次
  await initStreamListeners()
  await initPermissionListener()
  await initNotificationLayer()
  // 工作台持久化损坏回退提示(NFR-002)
  if (stateWasReset) {
    useNotifications().notifyTransient('工作台状态已重置')
  }
})
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <!-- 新壳：ActivityBar + 区域 v-show 切换（DOM 常驻，流式/滚动/监听零丢失） -->
  <div class="h-screen w-screen flex bg-background text-foreground" @contextmenu.prevent>
    <ActivityBar />
    <WorkbenchView v-show="activeSection === 'workbench'" class="flex-1 min-w-0" />
    <SessionsView v-show="activeSection === 'sessions'" class="flex-1 min-w-0" />
    <WorkshopView v-show="activeSection === 'workshop'" class="flex-1 min-w-0" />
    <AutomationView v-show="activeSection === 'automation'" class="flex-1 min-w-0" />
    <HomeView v-show="activeSection === 'home'" class="flex-1 min-w-0" />
    <SettingsView v-show="activeSection === 'settings'" class="flex-1 min-w-0" />

    <!-- 通知层:任何域可见(FR-006) -->
    <ToastStack />
    <!-- 全局确认弹窗 -->
    <ConfirmDialog />
  </div>
</template>
