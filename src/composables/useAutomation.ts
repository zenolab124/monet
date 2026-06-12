import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface HookEntry {
  event: string
  matcher: string | null
  command: string
  scope: string
  sourcePath: string
}

export interface HooksConfig {
  entries: HookEntry[]
  warnings: string[]
  homePath: string
}

export interface LastRun {
  timestamp: string
  exitCode: number
}

export interface HookStat {
  event: string
  command: string
  runs: number
  failures: number
  lastRun: LastRun | null
}

export interface HookRow extends HookEntry {
  runs: number | null
  failures: number | null
  lastRun: LastRun | null
  statsLoading: boolean
}

const config = ref<HooksConfig | null>(null)
const stats = ref<HookStat[] | null>(null)
const loadingConfig = ref(false)
const loadingStats = ref(false)
const errorConfig = ref<string | null>(null)
const errorStats = ref<string | null>(null)
let loaded = false

/** 将命令中的 $HOME / ${HOME} 替换为实际家目录路径（前端侧归一化，用于关联匹配） */
function normalizeCommand(cmd: string, home: string): string {
  return cmd.replace(/\$\{HOME\}/g, home).replace(/\$HOME/g, home)
}

/** 按 (event, normalized_command) 聚合统计 map */
function buildStatsMap(statsList: HookStat[]): Map<string, HookStat> {
  const map = new Map<string, HookStat>()
  for (const s of statsList) {
    map.set(`${s.event}\x00${s.command}`, s)
  }
  return map
}

/** 将配置行与统计关联，生成表格行 */
export function buildRows(entries: HookEntry[], statsList: HookStat[] | null, statsLoading: boolean, homePath: string): HookRow[] {
  const statsMap = statsList ? buildStatsMap(statsList) : null
  return entries.map(entry => {
    const key = `${entry.event}\x00${normalizeCommand(entry.command, homePath)}`
    const stat = statsMap?.get(key)
    return {
      ...entry,
      runs: stat?.runs ?? null,
      failures: stat?.failures ?? null,
      lastRun: stat?.lastRun ?? null,
      statsLoading,
    }
  })
}

async function loadConfig() {
  loadingConfig.value = true
  errorConfig.value = null
  try {
    config.value = await invoke<HooksConfig>('get_hooks_config')
  } catch (e) {
    errorConfig.value = String(e)
  } finally {
    loadingConfig.value = false
  }
}

async function loadStats() {
  loadingStats.value = true
  errorStats.value = null
  try {
    const list = await invoke<HookStat[]>('get_hooks_stats')
    stats.value = list
  } catch (e) {
    errorStats.value = String(e)
  } finally {
    loadingStats.value = false
  }
}

/** 首次进入自动化域或强制刷新时调用 */
async function ensureLoaded(force = false) {
  if (loaded && !force) return
  loaded = true
  // 并发调用，配置先到先渲染
  loadConfig()
  loadStats()
}

function refresh() {
  ensureLoaded(true)
}

export function useAutomation() {
  return {
    config,
    stats,
    loadingConfig,
    loadingStats,
    errorConfig,
    errorStats,
    ensureLoaded,
    refresh,
    buildRows,
  }
}
