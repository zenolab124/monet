<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDroppable } from '@dnd-kit/vue'
import { useWorkbench, setRightZoneWidth } from '@/composables/useWorkbench'
import { useNotifications } from '@/composables/useNotifications'
import WorkbenchColumnView from './WorkbenchColumn.vue'

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
    class="flex-1 min-w-0 h-full flex flex-row p-2.5"
    :class="{ 'drop-target-highlight': isDropTarget }"
  >
    <!-- 空态(FR-004) -->
    <div
      v-if="activeTab.columns.length === 0"
      class="flex-1 grid place-items-center text-xs text-muted-foreground"
    >
      {{ $t('workbench.columns.empty') }}
    </div>

    <template v-for="(col, i) in activeTab.columns" :key="col.id">
      <div
        class="min-w-0 h-full relative wb-col"
        :class="{
          'no-transition': dragging,
          'focus-ring': flashIndex === i,
        }"
        :style="{ flex: `${activeTab.columnSizes[i]} 1 0%` }"
      >
        <WorkbenchColumnView :column="col" :tab-id="activeTab.id" :index="i" />
      </div>

      <!-- 列间隙 10px:拖拽分隔命中区 9px 居中其内 -->
      <div v-if="i < activeTab.columns.length - 1" class="shrink-0 relative w-2.5">
        <div
          class="absolute top-0 bottom-0 left-1/2 -translate-x-1/2 w-9px cursor-col-resize z-10"
          @mousedown="onDividerMouseDown($event, i)"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.wb-col {
  transition: flex 200ms cubic-bezier(0.32, 0.72, 0, 1);
}
.wb-col.no-transition {
  transition: none;
}
/* dnd-kit: session 拖入右区时的容器高亮 */
.drop-target-highlight {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
  border-radius: 6px;
}
/* 幂等展开聚焦闪烁 */
.wb-col.focus-ring > :first-child {
  outline: 2px solid var(--ring);
  outline-offset: -1px;
}
</style>
