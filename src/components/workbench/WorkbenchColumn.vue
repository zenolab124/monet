<script setup lang="ts">
import { computed, ref } from 'vue'
import { useProjects } from '@/composables/useProjects'
import { useWorkbench, type WorkbenchColumn } from '@/composables/useWorkbench'
import { useSessionStream } from '@/composables/useStreaming'
import { useSessionStatus } from '@/composables/useSessionStatus'
import { useConfirm } from '@/composables/useConfirm'
import { displayTitle } from '@/types'
import SessionDetail from '../SessionDetail.vue'

/**
 * 右区单列(FR-004):列头(状态点+标题+收起+关闭) + 完整会话视图。
 * 列头可拖拽重排(FR-005 ④)。
 */
const props = defineProps<{
  column: WorkbenchColumn
  tabId: string
  index: number
}>()

const { projects } = useProjects()
const { collapseColumn, removeSession } = useWorkbench()
const { confirm } = useConfirm()

const sid = computed(() => props.column.sessionId)
const stream = useSessionStream(sid)
const status = useSessionStatus(sid)

const title = computed(() => {
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === props.column.sessionId)
    if (s) return displayTitle(s)
  }
  return props.column.sessionId.slice(0, 8)
})

/** 收起回左列:仍激活,流式继续,无确认(FR-004) */
function onCollapse() {
  collapseColumn(props.tabId, props.column.sessionId)
}

/** 关闭 = 退出工作台(行为同左列 ×,FR-002):流式中需确认 */
async function onClose() {
  if (stream.value.streaming) {
    const ok = await confirm('任务仍在进行,移出后仅通过通知提醒。确认移出工作台?', '移出')
    if (!ok) return
  }
  removeSession(props.column.sessionId)
}

// 列头拖拽(同容器内列重排)
const dragging = ref(false)

function onDragStart(e: DragEvent) {
  if (!e.dataTransfer) return
  e.dataTransfer.setData('text/cc-column', String(props.index))
  e.dataTransfer.effectAllowed = 'move'
  dragging.value = true
}

function onDragEnd() {
  dragging.value = false
}
</script>

<template>
  <div
    class="h-full flex flex-col bg-card border border-border rounded overflow-hidden"
    :class="dragging ? 'shadow-paper-lifted' : 'shadow-paper'"
  >
    <!-- 列头 -->
    <div
      class="shrink-0 flex items-center gap-2 px-3 py-2 border-b border-border cursor-grab active:cursor-grabbing"
      draggable="true"
      @dragstart="onDragStart"
      @dragend="onDragEnd"
    >
      <span
        class="w-1.5 h-1.5 rounded-full shrink-0"
        :class="[status.dotClass, { 'col-dot-pulse': status.pulse }]"
      />
      <span class="flex-1 min-w-0 truncate text-xs font-semibold">{{ title }}</span>
      <button
        class="w-5.5 h-5.5 grid place-items-center rounded text-muted-foreground hover:text-foreground hover:bg-muted shrink-0"
        title="收起回左列"
        @click="onCollapse"
      >
        <span class="i-carbon-chevron-left w-3 h-3" />
      </button>
      <button
        class="w-5.5 h-5.5 grid place-items-center rounded text-muted-foreground hover:text-destructive hover:bg-muted shrink-0"
        title="关闭(退出工作台)"
        @click="onClose"
      >
        <span class="i-carbon-close w-3 h-3" />
      </button>
    </div>

    <!-- 完整会话视图(独立输入/流式/权限) -->
    <div class="flex-1 min-h-0">
      <SessionDetail mode="workbench" :session-id="column.sessionId" />
    </div>
  </div>
</template>

<style scoped>
.col-dot-pulse {
  animation: col-pulse 1.6s ease-in-out infinite;
}
@keyframes col-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}
@media (prefers-reduced-motion: reduce) {
  .col-dot-pulse { animation: none; }
}
</style>
