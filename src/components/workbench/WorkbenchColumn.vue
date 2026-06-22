<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useProjects } from '@/composables/useProjects'
import { useWorkbench, type WorkbenchColumn } from '@/composables/useWorkbench'
import { useSessionStream } from '@/composables/useStreaming'
import { useSessionStatus } from '@/composables/useSessionStatus'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import { displayTitle } from '@/types'
import { useSessionMeta } from '@/composables/useSessionMeta'

const { getMeta } = useSessionMeta()
import SessionDetail from '../SessionDetail.vue'

const props = defineProps<{
  column: WorkbenchColumn
  tabId: string
  index: number
  handleRef?: (el: any) => void
}>()

const emit = defineEmits<{
  (e: 'startRace'): void
}>()

const { t } = useI18n()
const { projects } = useProjects()
const { collapseColumn, removeSession, removeRaceLane, draftCwd, findLane, state } = useWorkbench()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()

const tab = computed(() => state.value.tabs.find(t => t.id === props.tabId))
const lane = computed(() => tab.value ? findLane(tab.value, props.column.sessionId) : null)
const isRace = computed(() => !!lane.value)

const rcLoading = ref(false)

async function onToggleRC() {
  const enabling = !stream.value.rcActive
  rcLoading.value = true
  try {
    const session = projects.value.flatMap(p => p.sessions).find(s => s.id === props.column.sessionId)
    await invoke('toggle_remote_control', {
      sessionId: props.column.sessionId,
      cwd: session?.cwd ?? '',
      model: null,
      effort: null,
      channel: null,
      advisor: false,
      enabled: enabling,
      permissionMode: null,
    })
    stream.value.rcActive = enabling
    notifyTransient(enabling ? t('workbench.column.rcOpened') : t('workbench.column.rcClosed'))
  } catch (e) {
    notifyTransient(t('workbench.column.rcFailed'), String(e))
  } finally {
    rcLoading.value = false
  }
}

const sid = computed(() => props.column.sessionId)
const stream = useSessionStream(sid)
const status = useSessionStatus(sid)

const projectName = computed(() => {
  for (const p of projects.value) {
    if (p.sessions.some(s => s.id === props.column.sessionId))
      return p.display_path.split('/').pop() || p.display_path
  }
  const cwd = draftCwd(props.column.sessionId)
  return cwd ? cwd.split('/').pop() || cwd : null
})

const title = computed(() => {
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === props.column.sessionId)
    if (s) return displayTitle(s, getMeta(s.id)?.title)
  }
  if (draftCwd(props.column.sessionId)) return t('session.newSessionTitle')
  return props.column.sessionId.slice(0, 8)
})

function onCollapse() {
  collapseColumn(props.tabId, props.column.sessionId)
}

async function onClose() {
  if (stream.value.streaming) {
    const ok = await confirm(t('workbench.monitor.removeConfirm'), t('common.removeBrief'))
    if (!ok) return
  }
  removeSession(props.column.sessionId)
}

async function onCloseLane() {
  if (stream.value.streaming) {
    const ok = await confirm(t('workbench.monitor.removeConfirm'), t('common.removeBrief'))
    if (!ok) return
  }
  removeRaceLane(props.tabId, props.column.sessionId)
}

const isDragging = defineModel<boolean>('dragging', { default: false })
</script>

<template>
  <div
    class="h-full flex flex-col bg-card border border-border rounded overflow-hidden"
    :class="isDragging ? 'shadow-paper-lifted opacity-50' : 'shadow-paper'"
  >
    <!-- 列头 -->
    <div
      :ref="handleRef"
      class="shrink-0 flex items-center gap-2 px-3 py-2 border-b border-border cursor-grab active:cursor-grabbing touch-none"
    >
      <span
        class="w-1.5 h-1.5 rounded-full shrink-0"
        :class="[status.dotClass, { 'col-dot-pulse': status.pulse }]"
      />
      <template v-if="isRace">
        <span class="flex-1 min-w-0 truncate text-xs font-semibold">{{ lane!.label }}</span>
      </template>
      <template v-else>
        <span v-if="projectName" class="shrink-0 text-[10px] px-1.5 py-0.5 rounded leading-tight" style="color: var(--tag-foreground); background: var(--tag)">{{ projectName }}</span>
        <span class="flex-1 min-w-0 truncate text-xs font-semibold">{{ title }}</span>
      </template>
      <button
        :disabled="rcLoading"
        class="col-head-btn disabled:opacity-40"
        :class="stream.rcActive ? 'border-primary! text-primary!' : ''"
        :title="stream.rcActive ? $t('workbench.column.rcEnabled') : $t('workbench.column.rcEnable')"
        @pointerdown.stop
        @click.stop="onToggleRC"
      >
        <span class="i-carbon-remote-connection w-3 h-3" />
      </button>
      <!-- 普通模式:赛马 + 收起 + 关闭 -->
      <template v-if="!isRace">
        <button
          class="col-head-btn"
          :title="$t('workbench.race.startRace')"
          @pointerdown.stop
          @click.stop="emit('startRace')"
        >
          <span class="i-carbon-compare w-3 h-3" />
        </button>
        <button
          class="col-head-btn"
          :title="$t('workbench.column.collapseToRail')"
          @pointerdown.stop
          @click="onCollapse"
        >
          <span class="i-carbon-chevron-left w-3 h-3" />
        </button>
        <button
          class="col-head-btn hover:text-destructive!"
          :title="$t('workbench.column.closeExit')"
          @pointerdown.stop
          @click="onClose"
        >
          <span class="i-carbon-close w-3 h-3" />
        </button>
      </template>
      <!-- 赛马模式:关闭赛道 -->
      <button
        v-else
        class="col-head-btn hover:text-destructive!"
        :title="$t('workbench.race.closeLane')"
        @pointerdown.stop
        @click="onCloseLane"
      >
        <span class="i-carbon-close w-3 h-3" />
      </button>
    </div>

    <div class="flex-1 min-h-0">
      <SessionDetail mode="workbench" :session-id="column.sessionId" :hide-input="isRace" />
    </div>
  </div>
</template>

<style scoped>
.col-head-btn {
  width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  flex-shrink: 0;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  color: var(--muted-foreground);
  transition: color 0.15s, background-color 0.15s;
}
.col-head-btn:hover {
  color: var(--foreground);
  background: var(--muted);
}
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
