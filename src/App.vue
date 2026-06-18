<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
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
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions } from '@/composables/useSessions'
import { useUiState } from '@/composables/useUiState'
import { initPermissionListener } from '@/composables/usePermissionRequests'
import { initStreamListeners } from '@/composables/useStreaming'
import { initNotificationLayer, useNotifications } from '@/composables/useNotifications'
import { useRoutines } from '@/composables/useRoutines'
import { stateWasReset, useWorkbench } from '@/composables/useWorkbench'
import { DragDropProvider, DragOverlay } from '@dnd-kit/vue'

const { projects, loadProjects } = useProjects()
const { selectSession } = useSessions()
const { activeSection } = useUiState()
const { state, activeTab, pruneDrafts, reorderTabs, reorderSessions, moveSessionToTab, reorderColumns, expandSession } = useWorkbench()
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

// --- 拖拽实时排序 ---
const draggingSessionId = ref<string | null>(null)
const ghostHtml = ref('')

function onBeforeDragStart(event: any) {
  const sourceId = String(event.operation?.source?.id ?? '')
  if (sourceId.startsWith('session:')) {
    draggingSessionId.value = sourceId.slice(8)
    const el = event.operation?.source?.element
    if (el instanceof HTMLElement) {
      ghostHtml.value = el.outerHTML
    }
  }
}

function onWorkbenchDragOver(event: any) {
  const source = event.operation?.source
  const target = event.operation?.target
  if (!source || !target) return
  const sourceId = String(source.id ?? '')
  const targetId = String(target.id ?? '')
  if (sourceId.startsWith('session:') && targetId.startsWith('session-drop:')) {
    const fromSid = sourceId.slice(8)
    const toSid = targetId.slice(13)
    if (fromSid === toSid) return
    const tab = activeTab.value
    const fromIdx = tab.sessionIds.indexOf(fromSid)
    const toIdx = tab.sessionIds.indexOf(toSid)
    if (fromIdx >= 0 && toIdx >= 0 && fromIdx !== toIdx) {
      reorderSessions(tab.id, fromIdx, toIdx)
    }
  }
}

function onWorkbenchDragEnd(event: any) {
  draggingSessionId.value = null
  ghostHtml.value = ''
  if (event.canceled) return
  const source = event.operation?.source
  const target = event.operation?.target
  if (!source || !target) return

  const sourceId = String(source.id ?? '')
  const targetId = String(target.id ?? '')
  console.log('[dnd-end]', sourceId, '→', targetId)

  // Tab reorder (both start with "tab:")
  if (sourceId.startsWith('tab:') && targetId.startsWith('tab:')) {
    const fromIdx = state.value.tabs.findIndex(t => t.id === sourceId.slice(4))
    const toIdx = state.value.tabs.findIndex(t => t.id === targetId.slice(4))
    if (fromIdx >= 0 && toIdx >= 0 && fromIdx !== toIdx) {
      reorderTabs(fromIdx, toIdx > fromIdx ? toIdx : toIdx)
    }
    return
  }

  // Session dropped on tab
  if (sourceId.startsWith('session:') && targetId.startsWith('tab:')) {
    moveSessionToTab(sourceId.slice(8), targetId.slice(4))
    return
  }

  // Column reorder (both start with "col:")
  if (sourceId.startsWith('col:') && targetId.startsWith('col:')) {
    // col:{tabId}:{index}
    const [, tabId, fromStr] = sourceId.split(':')
    const [, , toStr] = targetId.split(':')
    const from = parseInt(fromStr)
    const to = parseInt(toStr)
    if (!isNaN(from) && !isNaN(to) && from !== to) {
      reorderColumns(tabId, from, to > from ? to : to)
    }
    return
  }

  // Session dropped on column area
  if (sourceId.startsWith('session:') && targetId.startsWith('col-zone:')) {
    const tabId = targetId.slice(9)
    const sessionId = sourceId.slice(8)
    expandSession(tabId, sessionId)
    return
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
    <DragDropProvider @before-drag-start="onBeforeDragStart" @drag-over="onWorkbenchDragOver" @drag-end="onWorkbenchDragEnd">
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

      <DragOverlay :disabled="!draggingSessionId">
        <div v-if="ghostHtml" v-html="ghostHtml" class="pointer-events-none opacity-85" />
      </DragOverlay>

      <ConfirmDialog />
    </DragDropProvider>
  </div>
</template>
