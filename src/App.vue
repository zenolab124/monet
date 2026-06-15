<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import TitleBar from '@/components/TitleBar.vue'
import TitleBarTools from '@/components/TitleBarTools.vue'
import ActivityBar from '@/components/ActivityBar.vue'
import WorkbenchTabs from '@/components/workbench/WorkbenchTabs.vue'
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
import { useRoutines } from '@/composables/useRoutines'
import { stateWasReset, useWorkbench } from '@/composables/useWorkbench'

const { projects, loadProjects } = useProjects()
const { selectSession } = useSessions()
const { activeSection } = useUiState()
const { pruneDrafts } = useWorkbench()
const { t } = useI18n()

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
  await useRoutines().initRoutineListener()
  // 工作台持久化损坏回退提示(NFR-002)
  if (stateWasReset) {
    useNotifications().notifyTransient(t('workbench.stateReset'))
  }
})
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="h-screen w-screen flex flex-col bg-background text-foreground" @contextmenu.prevent>
    <TitleBar>
      <template #leading>
        <WorkbenchTabs v-if="activeSection === 'workbench'" />
      </template>
      <template #trailing>
        <TitleBarTools />
      </template>
    </TitleBar>
    <div class="flex-1 flex min-h-0">
      <ActivityBar />
      <WorkbenchView v-show="activeSection === 'workbench'" class="flex-1 min-w-0" />
      <SessionsView v-show="activeSection === 'sessions'" class="flex-1 min-w-0" />
      <WorkshopView v-show="activeSection === 'workshop'" class="flex-1 min-w-0" />
      <AutomationView v-show="activeSection === 'automation'" class="flex-1 min-w-0" />
      <HomeView v-show="activeSection === 'home'" class="flex-1 min-w-0" />
      <SettingsView v-show="activeSection === 'settings'" class="flex-1 min-w-0" />
    </div>

    <ToastStack />
    <ConfirmDialog />
  </div>
</template>
