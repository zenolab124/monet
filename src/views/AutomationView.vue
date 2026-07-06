<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useUiState } from '@/composables/useUiState'
import { useAutomation, buildRows } from '@/composables/useAutomation'
import { useRoutines, type RoutineDefinition, type RoutineRow, type RoutineExecutionLog } from '@/composables/useRoutines'
import { renderMarkdownCached } from '@/composables/useMarkdown'
import RoutineForm from '@/components/automation/RoutineForm.vue'

const { t } = useI18n()
const { activeSection } = useUiState()
const {
  config, stats, loadingConfig, loadingStats,
  errorConfig, errorStats, ensureLoaded, refresh,
} = useAutomation()

const {
  routines: routineRows,
  loading: routinesLoading,
  error: routinesError,
  ensureLoaded: ensureRoutinesLoaded,
  refresh: refreshRoutines,
  deleteRoutine,
  updateRoutine,
  runNow,
  getRoutineLogs,
} = useRoutines()

// 首次进入自动化域加载数据
watch(activeSection, (s) => {
  if (s === 'automation') {
    ensureLoaded()
    ensureRoutinesLoaded()
  }
}, { immediate: true })

type AutoTab = 'hooks' | 'routines'
const autoTab = ref<AutoTab>('routines')

/** 表格行：配置为主体，统计异步填充 */
const rows = computed(() => {
  const cfg = config.value
  if (!cfg) return []
  return buildRows(cfg.entries, stats.value, loadingStats.value, cfg.homePath)
})

/** 格式化本地时刻 */
function formatTime(ts: string): string {
  try {
    const d = new Date(ts)
    return d.toLocaleString('zh-CN', {
      month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  } catch {
    return ts
  }
}

/** 「打开配置」失败的一次性提示 */
const openFailMsg = ref<string | null>(null)
let openFailTimer: ReturnType<typeof setTimeout> | undefined

async function openGlobalConfig() {
  const home = config.value?.homePath ?? ''
  const path = `${home}/.claude/settings.json`
  await openFile(path)
}

async function openFile(path: string) {
  openFailMsg.value = null
  try {
    await invoke('open_hooks_config', { path })
  } catch {
    openFailMsg.value = t('common.openFailed')
    clearTimeout(openFailTimer)
    openFailTimer = setTimeout(() => { openFailMsg.value = null }, 3000)
  }
}

const isLoading = computed(() => loadingConfig.value || loadingStats.value)

// --- 详情弹窗 ---
const detailPopup = ref<{ title: string; content: string } | null>(null)

// --- Routines UI ---
const showRoutineForm = ref<'new' | RoutineDefinition | null>(null)
const deletingId = ref<string | null>(null)

function onRoutineFormSaved() {
  showRoutineForm.value = null
}

async function onToggleRoutine(r: RoutineRow) {
  await updateRoutine(r.id, { enabled: !r.enabled })
}

async function onDeleteRoutine(r: RoutineRow) {
  deletingId.value = r.id
  try {
    await deleteRoutine(r.id)
  } finally {
    deletingId.value = null
  }
}

async function onRunNow(r: RoutineRow) {
  try {
    await runNow(r.id)
  } catch {
    // 已在运行中，忽略
  }
}

// --- 运行中已耗时：存在运行行时每秒 tick ---
const nowTick = ref(Date.now())
let tickTimer: number | null = null
watch(() => routineRows.value.some(r => r.isRunning), (hasRunning) => {
  if (hasRunning && tickTimer === null) {
    nowTick.value = Date.now()
    tickTimer = window.setInterval(() => { nowTick.value = Date.now() }, 1000)
  } else if (!hasRunning && tickTimer !== null) {
    window.clearInterval(tickTimer)
    tickTimer = null
  }
}, { immediate: true })
onBeforeUnmount(() => {
  if (tickTimer !== null) window.clearInterval(tickTimer)
})

function runningElapsed(r: RoutineRow): string {
  if (!r.runningStartedAt) return ''
  const s = Math.max(0, Math.floor((nowTick.value - new Date(r.runningStartedAt).getTime()) / 1000))
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`
}

/** 来源项目显示名：完整路径取末段 */
function sourceProjectName(r: RoutineRow): string {
  const p = r.source?.project
  if (!p) return ''
  return p.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? ''
}

// --- 日志弹窗 ---
const logPopup = ref<{ routine: RoutineRow; logs: RoutineExecutionLog[]; selected: number; loading: boolean } | null>(null)

async function openLogs(r: RoutineRow) {
  logPopup.value = { routine: r, logs: [], selected: 0, loading: true }
  try {
    const logs = await getRoutineLogs(r.id, 20)
    if (logPopup.value?.routine.id === r.id) {
      logPopup.value.logs = logs
      logPopup.value.loading = false
    }
  } catch {
    if (logPopup.value?.routine.id === r.id) {
      logPopup.value.loading = false
    }
  }
}

function selectLog(idx: number) {
  if (logPopup.value) logPopup.value.selected = idx
}

function formatDuration(start: string, end: string | null): string {
  if (!end) return ''
  const ms = new Date(end).getTime() - new Date(start).getTime()
  if (ms < 1000) return `${ms}ms`
  const s = Math.round(ms / 1000)
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  return `${m}m${s % 60}s`
}

function formatLogTime(ts: string): string {
  try {
    const d = new Date(ts)
    return d.toLocaleString('zh-CN', {
      month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit',
    })
  } catch {
    return ts
  }
}

function renderLogContent(log: RoutineExecutionLog): string {
  return renderMarkdownCached(log.stdout || '')
}
</script>

<template>
  <div class="h-full p-2.5">
    <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">

    <!-- 侧栏导航 -->
    <nav class="auto-nav">
      <div class="auto-nav-title">{{ $t('automation.title') }}</div>
      <button :class="['auto-nav-item', { active: autoTab === 'routines' }]" @click="autoTab = 'routines'">
        <span class="i-carbon-time w-3.5 h-3.5" />{{ $t('automation.scheduledTasks') }}
        <span class="auto-nav-count">{{ routineRows.length || '—' }}</span>
      </button>
      <button :class="['auto-nav-item', { active: autoTab === 'hooks' }]" @click="autoTab = 'hooks'">
        <span class="i-carbon-flow w-3.5 h-3.5" />Hooks
        <span class="auto-nav-count">{{ rows.length || '—' }}</span>
      </button>
    </nav>

    <!-- 内容区 -->
    <div class="flex-1 min-w-0 overflow-y-auto">
    <div class="content-area px-5 py-4">

      <!-- Hooks -->
      <section v-show="autoTab === 'hooks'">
        <div class="flex items-center gap-2 mb-2.5">
          <h2 class="sec-title mb-0">{{ $t('automation.hooksTitle') }}</h2>
          <div class="ml-auto flex items-center gap-1.5">
            <button class="icon-btn" :disabled="isLoading" @click="refresh">
              <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': isLoading }" />
            </button>
            <button class="auto-btn" @click="openGlobalConfig">
              <span class="i-carbon-settings w-3 h-3" />
              {{ $t('common.openConfig') }}
            </button>
          </div>
        </div>

        <!-- 配置加载中 -->
        <div v-if="loadingConfig && !config" class="py-8 text-center text-xs text-muted-foreground">
          {{ $t('common.loading') }}
        </div>

        <!-- 配置加载失败 -->
        <div v-else-if="errorConfig" class="py-8 text-center">
          <p class="text-xs text-destructive">{{ $t('common.configLoadFailed') }}</p>
          <button class="auto-btn mt-3" @click="refresh">{{ $t('common.retry') }}</button>
        </div>

        <!-- 空态：无任何配置 -->
        <div v-else-if="config && rows.length === 0" class="auto-empty">
          <p class="text-sm text-muted-foreground">{{ $t('automation.noHooks') }}</p>
        </div>

        <!-- 表格 -->
        <template v-else-if="config">
          <!-- 统计整体不可用提示 -->
          <p v-if="errorStats" class="mb-2 text-xs text-muted-foreground">
            <span class="i-carbon-warning-alt w-3 h-3 inline-block align-middle mr-0.5" />
            {{ $t('automation.statsUnavailable') }}
          </p>

          <div class="auto-table-wrap">
            <table class="auto-table">
              <thead>
                <tr>
                  <th>{{ $t('automation.event') }}</th>
                  <th>{{ $t('automation.action') }}</th>
                  <th>{{ $t('automation.scope') }}</th>
                  <th>{{ $t('automation.last7Days') }}</th>
                  <th>{{ $t('automation.lastResult') }}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(row, i) in rows" :key="i">
                  <!-- 事件列 -->
                  <td>
                    <div class="text-xs">{{ $t(`automation.events.${row.event}`, row.event) }}</div>
                    <div class="text-[10px] text-muted-foreground mt-0.5">{{ row.event }}<template v-if="row.matcher"> · {{ row.matcher }}</template></div>
                  </td>

                  <!-- 动作列 -->
                  <td>
                    <span class="wrap-cmd cursor-pointer" @click="detailPopup = { title: row.event, content: row.command }">{{ row.command }}</span>
                  </td>

                  <!-- 作用域列 -->
                  <td class="text-xs">{{ row.scope }}</td>

                  <!-- 近 7 天列 -->
                  <td class="text-xs">
                    <template v-if="errorStats">—</template>
                    <template v-else-if="row.statsLoading && row.runs === null">…</template>
                    <template v-else-if="row.runs === null">{{ $t('automation.noRunsIn7Days') }}</template>
                    <template v-else>
                      <span>{{ $t('automation.nTimes', { n: row.runs }) }}</span>
                      <span v-if="row.failures === 0" class="text-muted-foreground"> · {{ $t('automation.allSuccess') }}</span>
                      <span v-else class="text-destructive"> · {{ $t('automation.nFailed', { n: row.failures }) }}</span>
                    </template>
                  </td>

                  <!-- 上次结果列 -->
                  <td class="text-xs">
                    <template v-if="errorStats">—</template>
                    <template v-else-if="row.statsLoading && row.lastRun === null && row.runs === null">…</template>
                    <template v-else-if="!row.lastRun">—</template>
                    <template v-else>
                      <span :class="row.lastRun.exitCode === 0 ? 'text-success' : 'text-destructive'">
                        {{ row.lastRun.exitCode === 0 ? $t('automation.resultSuccess') : $t('automation.resultFailed') }}
                      </span>
                      <span class="text-muted-foreground"> · {{ formatTime(row.lastRun.timestamp) }}</span>
                    </template>
                  </td>

                  <!-- 打开图标（仅项目级） -->
                  <td class="text-center">
                    <button
                      v-if="row.scope !== $t('common.global')"
                      class="icon-btn icon-btn-sm"
                      v-tooltip="$t('common.openConfigFile')"
                      @click="openFile(row.sourcePath)"
                    >
                      <span class="i-carbon-document w-3 h-3 block" />
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </template>
      </section>

      <!-- 定时任务 -->
      <section v-show="autoTab === 'routines'">
        <div class="flex items-center gap-2 mb-2.5">
          <h2 class="sec-title mb-0">{{ $t('automation.routinesTitle') }}</h2>
          <div class="ml-auto flex items-center gap-1.5">
            <button class="icon-btn" :disabled="routinesLoading" @click="refreshRoutines">
              <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': routinesLoading }" />
            </button>
            <button class="auto-btn" @click="showRoutineForm = 'new'">
              <span class="i-carbon-add w-3 h-3" />
              {{ $t('automation.newRoutine') }}
            </button>
          </div>
        </div>

        <!-- 加载中 -->
        <div v-if="routinesLoading && !routineRows.length" class="py-8 text-center text-xs text-muted-foreground">
          {{ $t('common.loading') }}
        </div>

        <!-- 加载失败 -->
        <div v-else-if="routinesError" class="py-8 text-center">
          <p class="text-xs text-destructive">{{ $t('common.loadFailed') }}</p>
          <button class="auto-btn mt-3" @click="refreshRoutines">{{ $t('common.retry') }}</button>
        </div>

        <!-- 空态 -->
        <div v-else-if="!routineRows.length && !showRoutineForm" class="auto-empty">
          <p class="text-sm text-muted-foreground">{{ $t('automation.noRoutines') }}</p>
          <p class="text-xs text-muted-foreground mt-1">{{ $t('automation.routinesHint') }}</p>
        </div>

        <!-- 表格 -->
        <div v-if="routineRows.length" class="auto-table-wrap">
          <table class="auto-table routine-table">
            <thead>
              <tr>
                <th>{{ $t('automation.routineColumns.name') }}</th>
                <th>{{ $t('automation.routineColumns.schedule') }}</th>
                <th>{{ $t('automation.routineColumns.command') }}</th>
                <th>{{ $t('automation.routineColumns.status') }}</th>
                <th>{{ $t('automation.routineColumns.lastRun') }}</th>
                <th>{{ $t('automation.routineColumns.actions') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in routineRows" :key="r.id">
                <td class="text-xs font-medium">
                  {{ r.name }}
                  <div
                    v-if="r.source"
                    v-tooltip="r.source.kind === 'mcp' ? [r.source.project, r.source.client].filter(Boolean).join(' · ') : undefined"
                    class="text-[10px] text-muted-foreground font-normal mt-0.5"
                  >
                    <template v-if="r.source.kind === 'mcp'">
                      MCP<span v-if="sourceProjectName(r)"> · {{ sourceProjectName(r) }}</span>
                    </template>
                    <template v-else>{{ $t('automation.sourceUi') }}</template>
                  </div>
                </td>
                <td>
                  <code class="text-[11px]">{{ r.cronExpression }}</code>
                  <div v-if="r.originalText" v-tooltip="r.originalText" class="text-[10px] text-muted-foreground mt-0.5 truncate">{{ r.originalText }}</div>
                </td>
                <td>
                  <span class="truncate-cmd cursor-pointer" @click="detailPopup = { title: r.name, content: r.prompt }">{{ r.prompt }}</span>
                </td>
                <td class="text-xs whitespace-nowrap">
                  <span v-if="r.isRunning" class="inline-flex items-center gap-1 text-accent">
                    <span class="i-carbon-circle-dash w-3 h-3 animate-spin" />
                    {{ $t('common.running') }}
                    <span v-if="runningElapsed(r)" class="text-[10px] text-muted-foreground tabular-nums">{{ runningElapsed(r) }}</span>
                  </span>
                  <span v-else-if="r.enabled" class="text-success">{{ $t('common.enabled') }}</span>
                  <span v-else class="text-muted-foreground">{{ $t('common.paused') }}</span>
                </td>
                <td class="text-xs whitespace-nowrap">
                  <template v-if="!r.lastExecution">—</template>
                  <template v-else>
                    <span :class="r.lastExecution.exitCode === 0 ? 'text-success' : 'text-destructive'">
                      {{ r.lastExecution.exitCode === 0 ? '✓' : '✗' }}
                    </span>
                    <span class="text-muted-foreground">{{ formatTime(r.lastExecution.startedAt) }}</span>
                  </template>
                </td>
                <td>
                  <div class="routine-actions">
                    <button class="icon-btn icon-btn-sm" v-tooltip="r.enabled ? $t('automation.pause') : $t('automation.enable')" @click="onToggleRoutine(r)">
                      <span class="w-3 h-3 block" :class="r.enabled ? 'i-carbon-pause' : 'i-carbon-play'" />
                    </button>
                    <button class="icon-btn icon-btn-sm" v-tooltip="$t('common.edit')" @click="showRoutineForm = r">
                      <span class="i-carbon-edit w-3 h-3 block" />
                    </button>
                    <button class="icon-btn icon-btn-sm" v-tooltip="$t('automation.runNow')" :disabled="r.isRunning" @click="onRunNow(r)">
                      <span class="i-carbon-flash w-3 h-3 block" />
                    </button>
                    <button class="icon-btn icon-btn-sm" v-tooltip="$t('automation.routineColumns.lastRun')" @click="openLogs(r)">
                      <span class="i-carbon-report w-3 h-3 block" />
                    </button>
                    <button class="icon-btn icon-btn-sm" v-tooltip="$t('common.delete')" :disabled="deletingId === r.id" @click="onDeleteRoutine(r)">
                      <span class="i-carbon-trash-can w-3 h-3 block" />
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- 新建/编辑表单 -->
        <div v-if="showRoutineForm" class="mt-3">
          <RoutineForm
            :routine="showRoutineForm === 'new' ? null : showRoutineForm"
            @saved="onRoutineFormSaved"
            @cancel="showRoutineForm = null"
          />
        </div>
      </section>

    </div>
    </div>
    </div>

    <!-- 详情弹窗 -->
    <div
      v-if="detailPopup"
      class="fixed inset-0 z-70 grid place-items-center"
      style="background: rgba(70, 45, 20, 0.18)"
      @mousedown.self="detailPopup = null"
    >
      <div class="detail-popup">
        <div class="flex items-center justify-between mb-3">
          <span class="text-xs font-semibold">{{ detailPopup.title }}</span>
          <button class="icon-btn icon-btn-sm" @click="detailPopup = null">
            <span class="i-carbon-close w-3.5 h-3.5 block" />
          </button>
        </div>
        <pre class="text-xs text-foreground whitespace-pre-wrap break-all leading-relaxed">{{ detailPopup.content }}</pre>
      </div>
    </div>

    <!-- 日志弹窗 -->
    <div
      v-if="logPopup"
      class="fixed inset-0 z-70 grid place-items-center"
      style="background: rgba(70, 45, 20, 0.18)"
      @mousedown.self="logPopup = null"
    >
      <div class="log-popup">
        <!-- 标题栏 -->
        <div class="flex items-center justify-between mb-3 pb-2 border-b border-border">
          <span class="text-xs font-semibold">{{ $t('automation.logs.title', { name: logPopup.routine.name }) }}</span>
          <button class="icon-btn icon-btn-sm" @click="logPopup = null">
            <span class="i-carbon-close w-3.5 h-3.5 block" />
          </button>
        </div>

        <!-- 加载中 -->
        <div v-if="logPopup.loading" class="py-12 text-center text-xs text-muted-foreground">
          {{ $t('common.loading') }}
        </div>

        <!-- 空态 -->
        <div v-else-if="!logPopup.logs.length" class="py-12 text-center text-xs text-muted-foreground">
          {{ $t('automation.logs.noLogs') }}
        </div>

        <!-- 左右分栏 -->
        <div v-else class="log-body">
          <!-- 左侧：执行列表 -->
          <div class="log-list">
            <button
              v-for="(log, i) in logPopup.logs" :key="i"
              :class="['log-list-item', { active: logPopup.selected === i }]"
              @click="selectLog(i)"
            >
              <span class="log-status" :class="log.exitCode === 0 ? 'success' : log.exitCode == null ? 'running' : 'failed'" />
              <span class="text-xs">{{ formatLogTime(log.startedAt) }}</span>
              <span v-if="log.finishedAt" class="text-[10px] text-muted-foreground ml-auto">{{ formatDuration(log.startedAt, log.finishedAt) }}</span>
              <span v-else class="text-[10px] text-accent ml-auto">{{ $t('automation.logs.running') }}</span>
            </button>
          </div>

          <!-- 右侧：日志内容 -->
          <div class="log-content">
            <template v-if="logPopup.logs[logPopup.selected]">
              <!-- 元信息条 -->
              <div class="log-meta">
                <span>{{ formatLogTime(logPopup.logs[logPopup.selected].startedAt) }}</span>
                <span v-if="logPopup.logs[logPopup.selected].finishedAt">
                  {{ $t('automation.logs.duration', { duration: formatDuration(logPopup.logs[logPopup.selected].startedAt, logPopup.logs[logPopup.selected].finishedAt) }) }}
                </span>
                <span v-if="logPopup.logs[logPopup.selected].exitCode != null" :class="logPopup.logs[logPopup.selected].exitCode === 0 ? 'text-success' : 'text-destructive'">
                  {{ $t('automation.logs.exitCode', { code: logPopup.logs[logPopup.selected].exitCode }) }}
                </span>
              </div>

              <!-- stdout (Markdown) -->
              <div class="log-md prose prose-sm" v-html="renderLogContent(logPopup.logs[logPopup.selected])" />

              <!-- stderr -->
              <div v-if="logPopup.logs[logPopup.selected].stderr" class="mt-3">
                <div class="text-[10px] font-medium text-destructive mb-1">{{ $t('automation.logs.stderr') }}</div>
                <pre class="log-stderr">{{ logPopup.logs[logPopup.selected].stderr }}</pre>
              </div>
            </template>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 侧栏导航 */
.auto-nav {
  width: 160px;
  flex-shrink: 0;
  border-right: 1px solid var(--border);
  padding: 14px 8px;
  background: var(--background);
}
.auto-nav-title {
  padding: 0 8px;
  margin-bottom: 14px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--muted-foreground);
}
.auto-nav-item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 6px 10px;
  font-size: 12px;
  text-align: left;
  color: var(--muted-foreground);
  border-radius: var(--radius);
  cursor: pointer;
  margin-bottom: 2px;
  border: none;
  background: none;
}
.auto-nav-item:hover {
  background: var(--muted);
}
.auto-nav-item.active {
  color: var(--primary);
  background: var(--card);
  box-shadow: var(--shadow-paper);
}
.auto-nav-count {
  margin-left: auto;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.sec-title {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted-foreground);
  margin-bottom: 10px;
}

.auto-empty {
  text-align: center;
  padding: 40px 0;
}

.auto-table-wrap {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow-x: auto;
}

.auto-table {
  width: 100%;
  min-width: 720px;
  border-collapse: collapse;
  font-size: 12px;
}

.auto-table thead th {
  background: var(--muted);
  color: var(--muted-foreground);
  font-size: 11px;
  font-weight: 500;
  padding: 6px 10px;
  text-align: left;
  border-bottom: 1px solid var(--border);
}

.auto-table tbody tr {
  border-bottom: 1px solid var(--border);
  transition: background 0.1s;
}

.auto-table tbody tr:last-child {
  border-bottom: none;
}

.auto-table tbody tr:hover {
  background: var(--muted);
}

.auto-table td {
  padding: 10px;
  color: var(--foreground);
  vertical-align: middle;
}

.routine-table {
  display: grid;
  grid-template-columns: minmax(72px, 1fr) minmax(72px, 1fr) minmax(0, 3fr) auto auto auto;
  min-width: 0;
}

.routine-table thead,
.routine-table tbody {
  display: contents;
}

.routine-table tr {
  display: grid;
  grid-template-columns: subgrid;
  grid-column: 1 / -1;
  align-items: center;
}

.routine-table td:nth-child(3) {
  overflow: hidden;
}

.routine-table td:nth-child(4),
.routine-table td:nth-child(5),
.routine-table td:last-child {
  white-space: nowrap;
}

.routine-table td:last-child {
  padding: 6px 4px;
}

.routine-actions {
  display: flex;
  gap: 2px;
}

.truncate-cmd {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 11px;
  color: var(--muted-foreground);
}

.wrap-cmd {
  display: block;
  max-width: 380px;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 11px;
  color: var(--muted-foreground);
  word-break: break-all;
  line-height: 1.5;
}

.auto-note {
  font-size: 12px;
  color: var(--muted-foreground);
  line-height: 1.6;
  padding: 14px 16px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--muted);
}

.auto-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  padding: 3px 12px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--card);
  cursor: pointer;
}

.auto-btn:hover:not(:disabled) {
  box-shadow: var(--shadow-paper);
}

.auto-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.text-success {
  color: var(--success, #2d7d3a);
}

.detail-popup {
  width: 520px;
  max-width: 90vw;
  max-height: 70vh;
  overflow-y: auto;
  padding: 16px 20px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  box-shadow: var(--shadow-paper-lifted, 0 4px 16px rgba(0,0,0,0.12));
}

/* 日志弹窗 */
.log-popup {
  width: 780px;
  max-width: 92vw;
  height: 520px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  padding: 16px 20px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  box-shadow: var(--shadow-paper-lifted, 0 4px 16px rgba(0,0,0,0.12));
}

.log-body {
  flex: 1;
  display: flex;
  gap: 1px;
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
}

.log-list {
  width: 180px;
  flex-shrink: 0;
  overflow-y: auto;
  background: var(--muted);
  padding: 4px;
}

.log-list-item {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  background: none;
  border-radius: calc(var(--radius) - 2px);
  cursor: pointer;
  text-align: left;
}

.log-list-item:hover {
  background: var(--background);
}

.log-list-item.active {
  background: var(--card);
  box-shadow: var(--shadow-paper);
}

.log-status {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}
.log-status.success { background: var(--success, #2d7d3a); }
.log-status.failed { background: var(--destructive); }
.log-status.running { background: var(--accent); animation: pulse 1.5s infinite; }

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.log-content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 12px 16px;
  background: var(--card);
}

.log-meta {
  display: flex;
  gap: 12px;
  font-size: 10px;
  color: var(--muted-foreground);
  margin-bottom: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

.log-md {
  font-size: 12px;
  line-height: 1.7;
  color: var(--foreground);
}

.log-md :deep(table) {
  border-collapse: collapse;
  font-size: 11px;
  margin: 8px 0;
}
.log-md :deep(th),
.log-md :deep(td) {
  border: 1px solid var(--border);
  padding: 3px 8px;
}
.log-md :deep(th) {
  background: var(--muted);
  font-weight: 500;
}
.log-md :deep(p) {
  margin: 4px 0;
}
.log-md :deep(strong) {
  font-weight: 600;
}

.log-stderr {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 11px;
  color: var(--destructive);
  background: var(--muted);
  border-radius: calc(var(--radius) - 2px);
  padding: 8px 10px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 120px;
  overflow-y: auto;
}
</style>
