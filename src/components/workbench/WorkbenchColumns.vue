<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDroppable } from '@dnd-kit/vue'
import { useWorkbench, setRightZoneWidth } from '@/composables/useWorkbench'
import { useProjects } from '@/composables/useProjects'
import { useNotifications } from '@/composables/useNotifications'
import { useHorizontalWheelScroll } from '@/composables/useHorizontalWheelScroll'
import WorkbenchColumnView from './WorkbenchColumn.vue'
import SortableColumn from './SortableColumn.vue'

const {
  activeTab,
  updateColumnSize,
  expandSession,
  reorderColumns,
  focusColumnRequest,
  createRaceTab,
  setMinColumnWidth,
  minColumnWidth,
  draftCwd,
  registerFork,
  state,
  suppressColumnTransition,
} = useWorkbench()
const { t } = useI18n()
const { projects } = useProjects()
const { notifyTransient } = useNotifications()

function resolveSessionCwd(sessionId: string): string | undefined {
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === sessionId)
    if (s?.cwd) return s.cwd
  }
  return draftCwd(sessionId) ?? undefined
}

function onStartRace(sessionId: string) {
  const cwd = resolveSessionCwd(sessionId)
  if (!cwd) {
    notifyTransient(t('workbench.race.noCwd'))
    return
  }
  const isDraft = !!state.value.drafts[sessionId]
  const newSessionId = crypto.randomUUID()
  // 懒分叉:非草稿源登记分叉意图(落盘由 CLI --fork-session 完成);草稿源无历史,仅登记草稿
  if (!isDraft) {
    registerFork(newSessionId, sessionId, cwd)
  } else {
    state.value.drafts[newSessionId] = cwd
  }
  createRaceTab(sessionId, cwd, newSessionId)
}

const containerRef = ref<HTMLElement>()
const { isDropTarget } = useDroppable({ id: computed(() => 'col-zone:' + activeTab.value.id), element: containerRef })
const dragging = ref(false)

let zoneResizeObserver: ResizeObserver | null = null

useHorizontalWheelScroll(containerRef)

onMounted(() => {
  const el = containerRef.value
  if (!el) return
  setRightZoneWidth(el.clientWidth)
  zoneResizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      if (entry.contentRect.width > 0) {
        setRightZoneWidth(entry.contentRect.width)
      }
    }
  })
  zoneResizeObserver.observe(el)
})

onUnmounted(() => {
  zoneResizeObserver?.disconnect()
  zoneResizeObserver = null
})

const shiftDragging = ref(false)

/** 拖动第 index 条分隔线:像素级调整左列宽度;Shift 按住时调整全局最小列宽 */
function onDividerMouseDown(e: MouseEvent, index: number) {
  e.preventDefault()
  dragging.value = true
  const isShift = e.shiftKey
  shiftDragging.value = isShift

  const tab = activeTab.value
  const startX = e.clientX

  if (isShift) {
    const startMin = minColumnWidth.value
    const onMouseMove = (ev: MouseEvent) => {
      const delta = ev.clientX - startX
      const newMin = startMin + delta
      setMinColumnWidth(newMin)
      tab.columnSizes = tab.columnSizes.map(() => minColumnWidth.value)
    }
    const onMouseUp = () => {
      dragging.value = false
      shiftDragging.value = false
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
    }
    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
  } else {
    const startWidth = tab.columnSizes[index]
    const onMouseMove = (ev: MouseEvent) => {
      const delta = ev.clientX - startX
      updateColumnSize(tab.id, index, startWidth + delta)
    }
    const onMouseUp = () => {
      dragging.value = false
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
    }
    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
  }
}

// --- 幂等展开的滚动聚焦(FR-003:点击已展开卡 → 聚焦该列) ---

const flashIndex = ref(-1)

watch(focusColumnRequest, async (req) => {
  if (!req) return
  const idx = activeTab.value.columns.findIndex(c => c.sessionId === req.sessionId)
  if (idx < 0) return
  await nextTick()
  const colEl = containerRef.value?.querySelectorAll('.sortable-col')[idx] as HTMLElement | undefined
  colEl?.scrollIntoView({ behavior: 'smooth', inline: 'nearest', block: 'nearest' })
  flashIndex.value = idx
  window.setTimeout(() => {
    flashIndex.value = -1
  }, 900)
})
</script>

<template>
  <div class="flex-1 min-w-0 h-full relative">
    <div
      ref="containerRef"
      class="h-full flex flex-row p-2.5 gap-2.5 overflow-x-auto"
      :class="{ 'drop-target-highlight': isDropTarget }"
    >
      <!-- 空态(FR-004) -->
      <div
        v-if="activeTab.columns.length === 0"
        class="flex-1 grid place-items-center text-xs text-muted-foreground"
      >
        {{ $t('workbench.columns.empty') }}
      </div>

      <SortableColumn
        v-for="(col, i) in activeTab.columns"
        :key="col.id"
        :tab-id="activeTab.id"
        :index="i"
        :flex="activeTab.columnSizes[i]"
        :resizing="dragging || suppressColumnTransition"
      >
        <template #default="{ isDragging: colDragging, handleRef }">
          <WorkbenchColumnView :column="col" :tab-id="activeTab.id" :index="i" :dragging="colDragging" :handle-ref="handleRef" @start-race="onStartRace(col.sessionId)" />
          <!-- 列右边缘 resize 手柄（绝对定位，不参与 flex 布局） -->
          <div
            class="absolute top-0 bottom-0 -right-[7px] w-[14px] cursor-col-resize z-20"
            :class="{ 'divider-shift': shiftDragging }"
            @pointerdown.stop
            @mousedown="onDividerMouseDown($event, i)"
          />
        </template>
      </SortableColumn>
    </div>
  </div>
</template>

<style scoped>
.drop-target-highlight {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
  border-radius: 6px;
}
.divider-shift {
  background: color-mix(in srgb, var(--primary) 25%, transparent);
}
</style>
