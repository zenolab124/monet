<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useUiState } from '@/composables/useUiState'
import { useAutomation, buildRows } from '@/composables/useAutomation'

const { activeSection } = useUiState()
const {
  config, stats, loadingConfig, loadingStats,
  errorConfig, errorStats, ensureLoaded, refresh,
} = useAutomation()

// 首次进入自动化域加载数据
watch(activeSection, (s) => {
  if (s === 'automation') ensureLoaded()
}, { immediate: true })

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
    openFailMsg.value = '打开失败'
    clearTimeout(openFailTimer)
    openFailTimer = setTimeout(() => { openFailMsg.value = null }, 3000)
  }
}

const isLoading = computed(() => loadingConfig.value || loadingStats.value)
</script>

<template>
  <div class="h-full overflow-y-auto px-6.5 py-5" data-tauri-drag-region>
    <div class="max-w-190 mx-auto">

      <!-- 页头 -->
      <div class="flex items-center gap-2.5 mb-5">
        <h1 class="text-lg font-semibold">自动化</h1>
        <div class="ml-auto flex items-center gap-1.5">
          <span v-if="openFailMsg" class="text-xs text-destructive">{{ openFailMsg }}</span>
          <button class="auto-btn" :disabled="!config" @click="openGlobalConfig">打开配置</button>
          <button class="auto-btn" :disabled="isLoading" @click="refresh">
            <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': isLoading }" />
            刷新
          </button>
        </div>
      </div>

      <!-- Hooks 区 -->
      <section class="mb-6">
        <h2 class="sec-title">Hooks</h2>

        <!-- 配置加载中 -->
        <div v-if="loadingConfig && !config" class="py-8 text-center text-xs text-muted-foreground">
          加载中…
        </div>

        <!-- 配置加载失败 -->
        <div v-else-if="errorConfig" class="py-8 text-center">
          <p class="text-xs text-destructive">配置加载失败</p>
          <button class="auto-btn mt-3" @click="refresh">重试</button>
        </div>

        <!-- 空态：无任何配置 -->
        <div v-else-if="config && rows.length === 0" class="auto-empty">
          <p class="text-sm text-muted-foreground">未配置任何 Hook</p>
          <button class="auto-btn mt-3" @click="openGlobalConfig">打开配置</button>
        </div>

        <!-- 表格 -->
        <template v-else-if="config">
          <!-- 统计整体不可用提示 -->
          <p v-if="errorStats" class="mb-2 text-xs text-muted-foreground">
            <span class="i-carbon-warning-alt w-3 h-3 inline-block align-middle mr-0.5" />
            统计不可用
          </p>

          <div class="auto-table-wrap">
            <table class="auto-table">
              <thead>
                <tr>
                  <th>事件</th>
                  <th>动作</th>
                  <th>作用域</th>
                  <th>近 7 天</th>
                  <th>上次结果</th>
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
                    <template v-else-if="row.runs === null">7 天内无运行</template>
                    <template v-else>
                      <span>{{ row.runs }} 次</span>
                      <span v-if="row.failures === 0" class="text-muted-foreground"> · 全部成功</span>
                      <span v-else class="text-destructive"> · {{ row.failures }} 次失败</span>
                    </template>
                  </td>

                  <!-- 上次结果列 -->
                  <td class="text-xs">
                    <template v-if="errorStats">—</template>
                    <template v-else-if="row.statsLoading && row.lastRun === null && row.runs === null">…</template>
                    <template v-else-if="!row.lastRun">—</template>
                    <template v-else>
                      <span :class="row.lastRun.exitCode === 0 ? 'text-success' : 'text-destructive'">
                        {{ row.lastRun.exitCode === 0 ? '✓ 成功' : '✗ 失败' }}
                      </span>
                      <span class="text-muted-foreground"> · {{ formatTime(row.lastRun.timestamp) }}</span>
                    </template>
                  </td>

                  <!-- 打开图标（仅项目级） -->
                  <td class="text-center">
                    <button
                      v-if="row.scope !== '全局'"
                      class="auto-open-btn"
                      title="打开配置文件"
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

      <!-- 定时任务区（FR-004：固定说明空态） -->
      <section>
        <h2 class="sec-title">定时任务（Routines）</h2>
        <p class="auto-note">
          定时任务由 claude.ai 云端管理（/schedule 创建），本地无可读数据源，暂不支持展示。
        </p>
      </section>

    </div>
  </div>
</template>

<style scoped>
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
