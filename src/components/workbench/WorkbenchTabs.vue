<script setup lang="ts">
import { ref } from 'vue'
import { Menu } from '@tauri-apps/api/menu'
import { useWorkbench, type WorkbenchTab } from '@/composables/useWorkbench'
import { useConfirm } from '@/composables/useConfirm'

/**
 * 工作台 tab 条(FR-001):创建/重命名/关闭/拖拽重排;
 * 同时是卡片拖拽的跨台移动落点(FR-005 ②)。
 * 溢出时横向滚动,不换行、不下拉收纳。
 */
const { state, activeTab, setActiveTab, createTab, renameTab, closeTab, reorderTabs, moveSessionToTab } = useWorkbench()
const { confirm } = useConfirm()

// --- 重命名(双击 / 右键菜单触发;Esc 取消、失焦或 Enter 确认) ---

const editingTabId = ref<string | null>(null)
const editingName = ref('')

function startRename(tab: WorkbenchTab) {
  editingTabId.value = tab.id
  editingName.value = tab.name
}

/** v-for 内的 template ref 会被收集为数组,改用函数 ref 在挂载时聚焦全选 */
function focusEditInput(el: unknown) {
  if (el instanceof HTMLInputElement) {
    el.focus()
    el.select()
  }
}

function commitRename() {
  if (editingTabId.value) {
    renameTab(editingTabId.value, editingName.value)
  }
  editingTabId.value = null
}

function cancelRename() {
  editingTabId.value = null
}

// --- 关闭(含会话需确认;最后一个不可关) ---

async function requestClose(tab: WorkbenchTab) {
  if (state.value.tabs.length <= 1) return
  if (tab.sessionIds.length > 0) {
    const ok = await confirm(`${tab.sessionIds.length} 个会话将退出工作台`, '关闭')
    if (!ok) return
  }
  closeTab(tab.id)
}

// --- 右键菜单 ---

async function onContextMenu(e: MouseEvent, tab: WorkbenchTab) {
  e.preventDefault()
  const menu = await Menu.new({
    items: [
      {
        id: 'rename',
        text: '重命名',
        action: () => startRename(tab),
      },
      {
        id: 'close',
        text: '关闭',
        enabled: state.value.tabs.length > 1,
        action: () => void requestClose(tab),
      },
    ],
  })
  await menu.popup()
}

// --- tab 拖拽重排(FR-005 ①) + 接受监控卡拖入(②) ---

const dragTabIndex = ref<number | null>(null)
/** 落点指示:插入到 index 之前(-1 = 无) */
const dropIndicator = ref(-1)
/** 卡片悬停的目标 tab(高亮) */
const sessionHoverTabId = ref<string | null>(null)

function onTabDragStart(e: DragEvent, index: number) {
  if (!e.dataTransfer) return
  e.dataTransfer.setData('text/cc-tab', String(index))
  e.dataTransfer.effectAllowed = 'move'
  dragTabIndex.value = index
}

function onTabDragEnd() {
  dragTabIndex.value = null
  dropIndicator.value = -1
  sessionHoverTabId.value = null
}

function onTabDragOver(e: DragEvent, index: number, tab: WorkbenchTab) {
  if (!e.dataTransfer) return
  const types = e.dataTransfer.types
  if (types.includes('text/cc-session')) {
    // 监控卡拖到 tab 标签上:移入该工作台
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    sessionHoverTabId.value = tab.id
    dropIndicator.value = -1
  } else if (types.includes('text/cc-tab')) {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    // 鼠标在 tab 左半 → 插其前;右半 → 插其后
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const before = e.clientX < rect.left + rect.width / 2
    dropIndicator.value = before ? index : index + 1
    sessionHoverTabId.value = null
  }
}

function onTabDragLeave() {
  sessionHoverTabId.value = null
}

function onTabDrop(e: DragEvent, index: number, tab: WorkbenchTab) {
  if (!e.dataTransfer) return
  const sessionId = e.dataTransfer.getData('text/cc-session')
  if (sessionId) {
    e.preventDefault()
    moveSessionToTab(sessionId, tab.id)
    onTabDragEnd()
    return
  }
  const fromRaw = e.dataTransfer.getData('text/cc-tab')
  if (fromRaw !== '') {
    e.preventDefault()
    const from = parseInt(fromRaw, 10)
    let to = dropIndicator.value
    if (to < 0) to = index
    // splice 语义:从 from 移除后再插入,目标在其后时左移一位
    if (to > from) to -= 1
    reorderTabs(from, to)
    onTabDragEnd()
  }
}
</script>

<template>
  <div
    class="h-full flex items-center gap-0.5 pr-2 overflow-x-auto tabs-scroll"
  >
    <button
      v-for="(tab, i) in state.tabs"
      :key="tab.id"
      class="wb-tab"
      :class="{
        active: tab.id === activeTab.id,
        'session-hover': sessionHoverTabId === tab.id,
        'drop-before': dropIndicator === i,
        'drop-after': dropIndicator === i + 1 && i === state.tabs.length - 1,
      }"
      draggable="true"
      @click="setActiveTab(tab.id)"
      @dblclick="startRename(tab)"
      @contextmenu="onContextMenu($event, tab)"
      @dragstart="onTabDragStart($event, i)"
      @dragend="onTabDragEnd"
      @dragover="onTabDragOver($event, i, tab)"
      @dragleave="onTabDragLeave"
      @drop="onTabDrop($event, i, tab)"
    >
      <input
        v-if="editingTabId === tab.id"
        :ref="focusEditInput"
        v-model="editingName"
        class="w-24 bg-transparent border-none outline-none text-xs text-foreground"
        maxlength="20"
        @keydown.enter.prevent="commitRename"
        @keydown.esc.prevent="cancelRename"
        @blur="commitRename"
        @click.stop
      />
      <template v-else>
        <span class="truncate max-w-36">{{ tab.name }}</span>
        <span v-if="tab.sessionIds.length > 0" class="text-[10px] text-muted-foreground">{{ tab.sessionIds.length }}</span>
      </template>
    </button>

    <button
      class="wb-tab add shrink-0"
      title="新建工作台"
      @click="createTab()"
    >＋</button>

    <div class="flex-1 min-w-4 self-stretch" data-tauri-drag-region />
  </div>
</template>

<style scoped>
.wb-tab {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  padding: 2px 10px;
  border-radius: var(--radius);
  color: var(--muted-foreground);
  position: relative;
  white-space: nowrap;
  flex-shrink: 0;
  height: 22px;
}
.wb-tab:hover {
  background: var(--muted);
}
.wb-tab.active {
  background: var(--card);
  box-shadow: var(--shadow-paper);
  color: var(--foreground);
  font-weight: 500;
}
.wb-tab.add {
  padding: 4px 8px;
}
/* 卡片拖入悬停 */
.wb-tab.session-hover {
  background: var(--secondary);
  border-color: var(--primary);
}
/* tab 重排落点:2px primary 竖线 */
.wb-tab.drop-before::before,
.wb-tab.drop-after::after {
  content: '';
  position: absolute;
  top: 2px;
  bottom: 2px;
  width: 2px;
  border-radius: 1px;
  background: var(--primary);
}
.wb-tab.drop-before::before {
  left: -2px;
}
.wb-tab.drop-after::after {
  right: -2px;
}
/* tab 条横向滚动:细滚动条 */
.tabs-scroll {
  scrollbar-width: thin;
}
</style>
