<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDroppable } from '@dnd-kit/vue'
import { useWorkbench, setRightZoneWidth } from '@/composables/useWorkbench'
import { useNotifications } from '@/composables/useNotifications'
import WorkbenchColumnView from './WorkbenchColumn.vue'
import SortableColumn from './SortableColumn.vue'

/**
 * 右区多列布局(FR-004):浮起纸方案——四周边距与列间隙均 10px,
 * flex-grow 比例只作用于自由空间;
 * 同时承接两类拖拽落点(FR-005 ③卡片展开定位 / ④列头重排)。
 */
const {
  activeTab,
  updateColumnSize,
  expandSession,
  reorderColumns,
  focusColumnRequest,
  enforceColumnCapacity,
} = useWorkbench()
const { t } = useI18n()
const { notifyTransient, sessionTitle } = useNotifications()

const containerRef = ref<HTMLElement>()
const { isDropTarget } = useDroppable({ id: computed(() => 'col-zone:' + activeTab.value.id), element: containerRef })
const dragging = ref(false)

// 右区实测宽度回写状态层:动态列上限(每列 ≥ MIN_COLUMN_WIDTH)依赖它。
// 窗口缩小导致超员时自动收列(300ms 防抖:拖拽调窗过程中不连环触发)
let zoneResizeObserver: ResizeObserver | null = null
let capacityTimer: number | null = null

function scheduleCapacityCheck() {
  if (capacityTimer !== null) clearTimeout(capacityTimer)
  capacityTimer = window.setTimeout(() => {
    capacityTimer = null
    const collapsed = enforceColumnCapacity()
    if (collapsed.length > 0) {
      notifyTransient(t('workbench.columns.noSpace', { names: collapsed.map(sessionTitle).join('、') }))
    }
  }, 300)
}

onMounted(() => {
  const el = containerRef.value
  if (!el) return
  setRightZoneWidth(el.clientWidth)
  zoneResizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      if (entry.contentRect.width > 0) {
        setRightZoneWidth(entry.contentRect.width)
        scheduleCapacityCheck()
      }
    }
  })
  zoneResizeObserver.observe(el)
})

onUnmounted(() => {
  zoneResizeObserver?.disconnect()
  zoneResizeObserver = null
  if (capacityTimer !== null) clearTimeout(capacityTimer)
})

const PAD = 10
const GAP = 10

/** 拖动第 index 条分隔线(作用于当前 tab 的 columnSizes) */
function onDividerMouseDown(e: MouseEvent, index: number) {
  e.preventDefault()
  dragging.value = true
  const rect = containerRef.value?.getBoundingClientRect()
  if (!rect) return

  const tab = activeTab.value
  const freeWidth = rect.width - PAD * 2 - GAP * (tab.columns.length - 1)
  const prefix = tab.columnSizes.slice(0, index).reduce((s, v) => s + v, 0)
  const prefixLeft = prefix * freeWidth + PAD + GAP * index

  const onMouseMove = (ev: MouseEvent) => {
    const leftPx = ev.clientX - rect.left - prefixLeft
    updateColumnSize(tab.id, index, leftPx / freeWidth)
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
  const colEl = containerRef.value?.querySelectorAll('.wb-col')[idx] as HTMLElement | undefined
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
    class="flex-1 min-w-0 h-full flex flex-row p-2.5 gap-2.5"
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
      <template #default="{ isDragging: colDragging }">
        <WorkbenchColumnView :column="col" :tab-id="activeTab.id" :index="i" :dragging="colDragging" />
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
