<script setup lang="ts">
import { computed } from 'vue'
import { Menu } from '@tauri-apps/api/menu'
import { useWorkbench } from '@/composables/useWorkbench'
import { useProjects } from '@/composables/useProjects'
import { useUiState } from '@/composables/useUiState'
import { useNotifications } from '@/composables/useNotifications'
import MonitorCard from './MonitorCard.vue'

/**
 * 工作台左列(FR-003):当前 tab 全部会话的监控卡竖排(加入顺序,状态变化不重排)。
 * 底部「＋」格:从档案馆选择 / 新建会话(FR-002 进入口)。
 */
const { activeTab, createDraftSession } = useWorkbench()
const { projects } = useProjects()
const { switchSection } = useUiState()
const { notifyTransient } = useNotifications()

const expandedSet = computed(() => new Set(activeTab.value.columns.map(c => c.sessionId)))

const hint = computed(() => {
  const n = activeTab.value.sessionIds.length
  const m = activeTab.value.columns.length
  return n === 0 ? null : `${n} 个会话 · ${m} 个已展开`
})

/** ＋ 菜单:从档案馆选择 / 新建会话(子菜单按项目,应用内直开草稿卡,首条消息落盘) */
async function onAddClick() {
  const projectItems = projects.value.slice(0, 12).map(p => ({
    id: `new-${p.id}`,
    text: p.display_path.split('/').pop() || p.display_path,
    action: () => {
      createDraftSession(p.display_path)
      notifyTransient('新会话已就绪', '输入第一条消息开始对话')
    },
  }))
  const menu = await Menu.new({
    items: [
      {
        id: 'from-archive',
        text: '从档案馆选择',
        action: () => switchSection('sessions'),
      },
      {
        id: 'new-session',
        text: '新建会话',
        enabled: projectItems.length > 0,
        items: projectItems,
      },
    ],
  })
  await menu.popup()
}
</script>

<template>
  <aside class="w-64 shrink-0 border-r border-border overflow-y-auto p-2.5 flex flex-col gap-2">
    <div v-if="hint" class="px-0.5 text-[10.5px] text-muted-foreground shrink-0">{{ hint }}</div>

    <MonitorCard
      v-for="sid in activeTab.sessionIds"
      :key="sid"
      :session-id="sid"
      :expanded="expandedSet.has(sid)"
    />

    <!-- 空态(FR-001) -->
    <div
      v-if="activeTab.sessionIds.length === 0"
      class="px-2 py-6 text-center text-xs text-muted-foreground leading-relaxed"
    >
      从档案馆打开会话，<br />或点击下方 ＋ 新建
    </div>

    <!-- ＋ 格 -->
    <button
      class="shrink-0 min-h-12 border border-dashed border-border rounded text-xs text-muted-foreground
             flex items-center justify-center gap-1.5 hover:text-primary hover:border-primary transition-colors"
      @click="onAddClick"
    >
      <span class="text-sm">＋</span>
      <span>打开会话 / 新任务</span>
    </button>
  </aside>
</template>
