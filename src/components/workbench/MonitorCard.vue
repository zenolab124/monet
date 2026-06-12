<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue'
import { useProjects } from '@/composables/useProjects'
import { useWorkbench } from '@/composables/useWorkbench'
import { useSessionStream, useStreaming } from '@/composables/useStreaming'
import { useSessionStatus } from '@/composables/useSessionStatus'
import { queueForSession, usePermissionRequests } from '@/composables/usePermissionRequests'
import { useNotifications } from '@/composables/useNotifications'
import { useConfirm } from '@/composables/useConfirm'
import { displayTitle, formatTokens, relativeTime } from '@/types'

/**
 * 左列监控卡(FR-003):状态行 / 标题 / 尾部区 / 就地决策条 / meta 行。
 * 只消费 stream.tail 等轻量字段,不 mount 消息流组件树(NFR-001 渲染分级)。
 */
const props = defineProps<{
  sessionId: string
  expanded: boolean
}>()

const sid = computed(() => props.sessionId)

const { projects } = useProjects()
const { activeTab, expandSession, removeSession, flashSessionId, draftCwd } = useWorkbench()
const { retrySession } = useStreaming()
const { respondRequest } = usePermissionRequests()
const { notifyTransient, sessionTitle, dismissError } = useNotifications()
const { confirm } = useConfirm()

const stream = useSessionStream(sid)
const status = useSessionStatus(sid)
const perms = queueForSession(sid)
const headPerm = computed(() => perms.value[0] ?? null)

const summary = computed(() => {
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === props.sessionId)
    if (s) return { summary: s, projectName: p.display_path.split('/').pop() || p.display_path }
  }
  return null
})

/** 应用内新建未落盘的草稿:标题与项目名占位 */
const draft = computed(() => {
  if (summary.value) return null
  const cwd = draftCwd(props.sessionId)
  return cwd ? { projectName: cwd.split('/').pop() || cwd } : null
})

const title = computed(() =>
  summary.value ? displayTitle(summary.value.summary)
  : draft.value ? '新会话'
  : props.sessionId.slice(0, 8),
)

// --- 秒级时钟:流式持续时间显示用,空闲时停表 ---

const now = ref(Date.now())
let timer: number | null = null

watch(
  () => stream.value.streaming,
  (active) => {
    if (active && timer === null) {
      timer = window.setInterval(() => {
        now.value = Date.now()
      }, 1000)
    } else if (!active && timer !== null) {
      clearInterval(timer)
      timer = null
    }
  },
  { immediate: true },
)

onUnmounted(() => {
  if (timer !== null) clearInterval(timer)
})

/** 决策条单行摘要:工具名 + 目标 */
const permSummary = computed(() => {
  const p = headPerm.value
  if (!p) return ''
  for (const k of ['file_path', 'command', 'url', 'pattern']) {
    const v = p.input[k]
    if (typeof v === 'string' && v) return `${p.toolName} · ${v.split('\n')[0]}`
  }
  return p.toolName
})

/** 出错重试条(stream 源且有可重发消息) */
const canRetry = computed(() => status.value.key === 'error' && !!stream.value.lastSent)

// --- meta:持续时间(运行中)或最后活动时间 ---

const durationText = computed(() => {
  if (stream.value.streaming && stream.value.startedAt) {
    const mins = Math.floor((now.value - stream.value.startedAt) / 60_000)
    return mins < 1 ? '刚开始' : `${mins} 分钟`
  }
  return summary.value ? relativeTime(summary.value.summary.last_modified) : ''
})

const tokenText = computed(() => {
  const t = summary.value?.summary.total_tokens
  if (!t) return null
  const total = t.input_tokens + t.output_tokens + t.cache_creation_input_tokens + t.cache_read_input_tokens
  return total > 0 ? formatTokens(total) : null
})

// --- 交互 ---

/** 点击卡片:未展开→展开;已展开→右区滚动聚焦(幂等) */
function onCardClick() {
  const result = expandSession(activeTab.value.id, props.sessionId)
  if (result.collapsedSessionIds.length > 0) {
    notifyTransient(`已收起:${result.collapsedSessionIds.map(sessionTitle).join('、')}`)
  }
}

/** ×:退出工作台。流式中需确认(退出≠终止,流在后台继续直至落盘) */
async function onClose() {
  if (stream.value.streaming) {
    const ok = await confirm('任务仍在进行,移出后仅通过通知提醒。确认移出工作台?', '移出')
    if (!ok) return
  }
  removeSession(props.sessionId)
}

async function onAllow() {
  const p = headPerm.value
  if (p) await respondRequest(p.requestId, 'allow_once')
}

async function onDeny() {
  const p = headPerm.value
  if (p) await respondRequest(p.requestId, 'deny')
}

async function onRetry() {
  const ok = await retrySession(props.sessionId)
  if (ok) dismissError(props.sessionId)
}

// --- 拖拽(FR-005:拖至 tab 跨台移动 / 拖至右区展开定位) ---

const dragging = ref(false)

function onDragStart(e: DragEvent) {
  if (!e.dataTransfer) return
  e.dataTransfer.setData('text/cc-session', props.sessionId)
  e.dataTransfer.effectAllowed = 'move'
  dragging.value = true
}

function onDragEnd() {
  dragging.value = false
}
</script>

<template>
  <div
    class="monitor-card bg-card border border-border rounded shadow-paper overflow-hidden cursor-pointer transition-shadow"
    :class="{
      'edge-accent': status.edge === 'accent',
      'edge-destructive': status.edge === 'destructive',
      'shadow-paper-lifted': dragging,
      'flash-once': flashSessionId === sessionId,
    }"
    draggable="true"
    @click="onCardClick"
    @dragstart="onDragStart"
    @dragend="onDragEnd"
  >
    <!-- 状态行 -->
    <div class="px-2.5 pt-2 flex items-center gap-1.5 text-[10.5px] text-muted-foreground">
      <span
        class="w-1.5 h-1.5 rounded-full shrink-0"
        :class="[status.dotClass, { 'dot-pulse': status.pulse }]"
      />
      <span
        class="font-semibold"
        :class="{
          'text-primary': status.dotClass === 'bg-primary',
          'text-accent': status.key === 'waiting_permission',
          'text-destructive': status.key === 'error',
        }"
      >{{ status.label }}</span>
      <span
        v-if="expanded"
        class="px-1 text-[9.5px] border border-primary text-primary rounded-sm shrink-0"
      >已展开</span>
      <button
        class="ml-auto w-4 h-4 grid place-items-center rounded-sm text-muted-foreground hover:text-destructive hover:bg-muted shrink-0"
        title="退出工作台"
        @click.stop="onClose"
      >
        <span class="i-carbon-close w-3 h-3" />
      </button>
    </div>

    <!-- 标题 -->
    <div class="px-2.5 mt-0.5 text-xs font-semibold truncate">{{ title }}</div>

    <!-- 尾部区:最近输出末 2-3 行(150ms 节流;流式中末行带光标) -->
    <div class="mx-2.5 my-1.5 px-2 py-1.5 bg-background border border-border rounded text-[11px] leading-relaxed text-muted-foreground min-h-9">
      <template v-if="stream.tail.length > 0">
        <div
          v-for="(line, i) in stream.tail"
          :key="i"
          class="truncate"
          :class="{
            'font-mono text-[10.5px]': line.kind === 'tool',
            'text-destructive': line.kind === 'error',
            'text-foreground': i === stream.tail.length - 1 && line.kind === 'text',
          }"
        >
          {{ line.text }}<span
            v-if="stream.streaming && i === stream.tail.length - 1"
            class="tail-caret"
          />
        </div>
      </template>
      <div v-else class="truncate">
        {{ stream.streaming ? '正在启动…' : '暂无实时输出' }}<span v-if="stream.streaming" class="tail-caret" />
      </div>
    </div>

    <!-- 就地决策条:等待权限 -->
    <div
      v-if="headPerm"
      class="mx-2.5 mb-1.5 px-2 py-1 border border-border rounded bg-popover flex items-center gap-1.5 text-[11px] decision-accent"
      @click.stop
    >
      <span class="flex-1 min-w-0 truncate font-mono text-[10.5px] text-muted-foreground">{{ permSummary }}</span>
      <button
        class="px-2 py-0.5 text-[10.5px] rounded bg-primary text-primary-foreground shrink-0"
        @click.stop="onAllow"
      >允许</button>
      <button
        class="px-2 py-0.5 text-[10.5px] rounded border border-border text-muted-foreground shrink-0"
        @click.stop="onDeny"
      >拒</button>
    </div>

    <!-- 就地决策条:出错重试 -->
    <div
      v-else-if="canRetry"
      class="mx-2.5 mb-1.5 px-2 py-1 border border-border rounded bg-popover flex items-center gap-1.5 text-[11px] decision-destructive"
      @click.stop
    >
      <span class="flex-1 min-w-0 truncate text-muted-foreground">任务已停住</span>
      <button
        class="px-2 py-0.5 text-[10.5px] rounded bg-primary text-primary-foreground shrink-0"
        @click.stop="onRetry"
      >重试</button>
    </div>

    <!-- meta 行:项目名 + 持续/最后活动时间 + token -->
    <div class="px-2.5 pb-2 flex items-center gap-2.5 text-[10px] text-muted-foreground tabular-nums">
      <span class="truncate">{{ summary?.projectName ?? draft?.projectName ?? '—' }}</span>
      <span class="shrink-0">{{ durationText }}</span>
      <span v-if="tokenText" class="shrink-0">{{ tokenText }}</span>
    </div>
  </div>
</template>

<style scoped>
.monitor-card:hover {
  box-shadow: var(--shadow-paper-lifted);
}
/* 状态左边框(3px 语义色) */
.edge-accent {
  border-left: 3px solid var(--accent);
}
.edge-destructive {
  border-left: 3px solid var(--destructive);
}
/* 决策条左侧 2px 语义线 */
.decision-accent {
  border-left: 2px solid var(--accent);
}
.decision-destructive {
  border-left: 2px solid var(--destructive);
}
/* 运行态脉冲 */
.dot-pulse {
  animation: mc-pulse 1.6s ease-in-out infinite;
}
@keyframes mc-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}
/* 流式尾部光标 */
.tail-caret {
  display: inline-block;
  width: 2px;
  height: 1em;
  background: var(--foreground);
  vertical-align: text-bottom;
  margin-left: 1px;
  animation: mc-blink 1s steps(2) infinite;
}
@keyframes mc-blink {
  50% { opacity: 0; }
}
/* 重复打开高亮:背景闪烁一次(FR-002) */
.flash-once {
  animation: mc-flash 1s ease-out 1;
}
@keyframes mc-flash {
  0%, 60% { background: var(--secondary); }
  100% { background: var(--card); }
}
@media (prefers-reduced-motion: reduce) {
  .dot-pulse, .tail-caret, .flash-once {
    animation: none;
  }
}
</style>
