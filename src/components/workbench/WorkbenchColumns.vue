<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDroppable } from '@dnd-kit/vue'
import { invoke } from '@tauri-apps/api/core'
import { useWorkbench, setRightZoneWidth } from '@/composables/useWorkbench'
import { useProjects } from '@/composables/useProjects'
import { useNotifications } from '@/composables/useNotifications'
import WorkbenchColumnView from './WorkbenchColumn.vue'
import SortableColumn from './SortableColumn.vue'

const {
  activeTab,
  updateColumnSize,
  expandSession,
  reorderColumns,
  focusColumnRequest,
  createRaceTab,
  draftCwd,
  state,
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

async function onStartRace(sessionId: string) {
  const cwd = resolveSessionCwd(sessionId)
  if (!cwd) {
    notifyTransient(t('workbench.race.noCwd'))
    return
  }
  const isDraft = !!state.value.drafts[sessionId]
  const newSessionId = crypto.randomUUID()
  try {
    if (!isDraft) {
      await invoke('fork_session', {
        sourceSessionId: sessionId,
        newSessionId,
        cwd,
      })
    } else {
      state.value.drafts[newSessionId] = cwd
    }
    createRaceTab(sessionId, cwd, newSessionId)
  } catch (e) {
    notifyTransient(t('workbench.race.forkFailed'), String(e))
  }
}

const containerRef = ref<HTMLElement>()
const { isDropTarget } = useDroppable({ id: computed(() => 'col-zone:' + activeTab.value.id), element: containerRef })
const dragging = ref(false)

let zoneResizeObserver: ResizeObserver | null = null

function onWheelCapture(e: WheelEvent) {
  const el = containerRef.value
  if (!el || el.scrollWidth <= el.clientWidth) return
  if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return
  e.preventDefault()
  el.scrollLeft += e.deltaX
}

onMounted(() => {
  const el = containerRef.value
  if (!el) return
  setRightZoneWidth(el.clientWidth)
  el.addEventListener('wheel', onWheelCapture, { capture: true, passive: false })
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
  containerRef.value?.removeEventListener('wheel', onWheelCapture, { capture: true } as EventListenerOptions)
  zoneResizeObserver?.disconnect()
  zoneResizeObserver = null
})

/** 拖动第 index 条分隔线:像素级调整左列宽度 */
function onDividerMouseDown(e: MouseEvent, index: number) {
  e.preventDefault()
  dragging.value = true

  const tab = activeTab.value
  const startX = e.clientX
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
  <div
    ref="containerRef"
    class="flex-1 min-w-0 h-full flex flex-row p-2.5 gap-2.5 overflow-x-auto"
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
    >
      <template #default="{ isDragging: colDragging, handleRef }">
        <WorkbenchColumnView :column="col" :tab-id="activeTab.id" :index="i" :dragging="colDragging" :handle-ref="handleRef" @start-race="onStartRace(col.sessionId)" />
        <!-- 列间 resize 手柄（绝对定位在右边界，不参与 flex 布局） -->
        <div
          v-if="i < activeTab.columns.length - 1"
          class="absolute top-0 bottom-0 -right-[7px] w-[14px] cursor-col-resize z-20"
          @pointerdown.stop
          @mousedown="onDividerMouseDown($event, i)"
        />
      </template>
    </SortableColumn>
  </div>
</template>

<style scoped>
.drop-target-highlight {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
  border-radius: 6px;
}
</style>
