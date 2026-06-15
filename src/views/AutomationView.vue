<script setup lang="ts">
import { computed, ref, watch, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useUiState } from '@/composables/useUiState'
import { useAutomation, buildRows } from '@/composables/useAutomation'
import { useRoutines, type RoutineDefinition, type RoutineRow } from '@/composables/useRoutines'
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
} = useRoutines()

// 首次进入自动化域加载数据
watch(activeSection, (s) => {
  if (s === 'automation') {
    ensureLoaded()
    ensureRoutinesLoaded()
  }
}, { immediate: true })

type AutoTab = 'hooks' | 'routines'
const autoTab = ref<AutoTab>('hooks')

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
</script>

<template>
  <div class="h-full p-2.5">
    <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">

    <!-- 侧栏导航 -->
    <nav class="auto-nav">
      <div class="auto-nav-title">{{ $t('automation.title') }}</div>
      <button :class="['auto-nav-item', { active: autoTab === 'hooks' }]" @click="autoTab = 'hooks'">
        <span class="i-carbon-flow w-3.5 h-3.5" />Hooks
        <span class="auto-nav-count">{{ rows.length || '—' }}</span>
      </button>
      <button :class="['auto-nav-item', { active: autoTab === 'routines' }]" @click="autoTab = 'routines'">
        <span class="i-carbon-time w-3.5 h-3.5" />{{ $t('automation.scheduledTasks') }}
        <span class="auto-nav-count">{{ routineRows.length || '—' }}</span>
      </button>
    </nav>

    <!-- 内容区 -->
    <div class="flex-1 min-w-0 overflow-y-auto">
    <div class="content-area px-5 py-4">

      <!-- Hooks -->
      <section v-show="autoTab === 'hooks'">

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
          <button class="auto-btn mt-3" @click="openGlobalConfig">{{ $t('common.openConfig') }}</button>
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
                    <code class="text-[11px]">{{ row.event }}</code>
                    <div v-if="row.matcher" class="text-[10px] text-muted-foreground mt-0.5">{{ row.matcher }}</div>
                  </td>

                  <!-- 动作列 -->
                  <td>
                    <span class="truncate-cmd" :title="row.command">{{ row.command }}</span>
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
                      class="auto-open-btn"
                      :title="$t('common.openConfigFile')"
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
            <button class="auto-btn" :disabled="routinesLoading" @click="refreshRoutines">
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
          <table class="auto-table">
            <thead>
              <tr>
                <th>{{ $t('automation.routineColumns.name') }}</th>
                <th>{{ $t('automation.routineColumns.schedule') }}</th>
                <th>{{ $t('automation.routineColumns.command') }}</th>
                <th>{{ $t('automation.routineColumns.status') }}</th>
                <th>{{ $t('automation.routineColumns.lastRun') }}</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in routineRows" :key="r.id">
                <td class="text-xs font-medium">{{ r.name }}</td>
                <td>
                  <code class="text-[11px]">{{ r.cronExpression }}</code>
                  <div v-if="r.originalText" class="text-[10px] text-muted-foreground mt-0.5">{{ r.originalText }}</div>
                </td>
                <td>
                  <span class="truncate-cmd" :title="r.prompt">{{ r.prompt }}</span>
                </td>
                <td class="text-xs">
                  <span v-if="r.isRunning" class="text-accent">{{ $t('common.running') }}</span>
                  <span v-else-if="r.enabled" class="text-success">{{ $t('common.enabled') }}</span>
                  <span v-else class="text-muted-foreground">{{ $t('common.paused') }}</span>
                </td>
                <td class="text-xs">
                  <template v-if="!r.lastExecution">—</template>
                  <template v-else>
                    <span :class="r.lastExecution.exitCode === 0 ? 'text-success' : 'text-destructive'">
                      {{ r.lastExecution.exitCode === 0 ? '✓' : '✗' }}
                    </span>
                    <span class="text-muted-foreground"> {{ formatTime(r.lastExecution.startedAt) }}</span>
                  </template>
                </td>
                <td>
                  <div class="flex items-center gap-0.5">
                    <button
                      class="auto-open-btn" :title="r.enabled ? $t('automation.pause') : $t('automation.enable')"
                      @click="onToggleRoutine(r)"
                    >
                      <span class="w-3 h-3 block" :class="r.enabled ? 'i-carbon-pause' : 'i-carbon-play'" />
                    </button>
                    <button class="auto-open-btn" :title="$t('common.edit')" @click="showRoutineForm = r">
                      <span class="i-carbon-edit w-3 h-3 block" />
                    </button>
                    <button
                      class="auto-open-btn" :title="$t('automation.runNow')"
                      :disabled="r.isRunning"
                      @click="onRunNow(r)"
                    >
                      <span class="i-carbon-flash w-3 h-3 block" />
                    </button>
                    <button
                      class="auto-open-btn" :title="$t('common.delete')"
                      :disabled="deletingId === r.id"
                      @click="onDeleteRoutine(r)"
                    >
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
  overflow: hidden;
}

.auto-table {
  width: 100%;
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
  padding: 7px 10px;
  color: var(--foreground);
  vertical-align: top;
}

.truncate-cmd {
  display: block;
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 11px;
  color: var(--muted-foreground);
}

.auto-open-btn {
  width: 22px;
  height: 22px;
  border-radius: var(--radius);
  display: grid;
  place-items: center;
  color: var(--muted-foreground);
  transition: color 0.1s, background 0.1s;
}

.auto-open-btn:hover {
  color: var(--foreground);
  background: var(--muted);
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
</style>
