import { computed, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/** 远端主机镜像同步：前台每 60 秒一轮全量索引，进度经 remote-sync-progress 推送，可取消 */

export interface SyncReport { listed: number; fetched: number; missing: number; cancelled: boolean; errors: string[] }
export interface SyncProgress { alias: string; instanceId: string; done: number; total: number; running: boolean }
interface RemoteHostEntry { alias: string; enabled: boolean }

const INDEX_INTERVAL_MS = 60_000

const progress = reactive<Record<string, SyncProgress>>({})
let initialized = false
let timer: number | null = null

async function init() {
  if (initialized) return
  initialized = true
  await listen<SyncProgress>('remote-sync-progress', event => {
    const payload = event.payload
    if (payload.running) progress[payload.alias] = payload
    else delete progress[payload.alias]
  })
}

/** 同步一台主机；已在同步中时后端拒绝，这里原样抛出 */
async function syncHost(alias: string): Promise<SyncReport> {
  await init()
  return invoke<SyncReport>('sync_remote_host', { alias })
}

async function cancelSync(alias: string): Promise<boolean> {
  return invoke<boolean>('cancel_remote_sync', { alias })
}

/** 同步全部已启用主机；单台失败（离线、正在同步）不影响其余 */
async function syncAll(): Promise<void> {
  let hosts: RemoteHostEntry[]
  try {
    hosts = await invoke<RemoteHostEntry[]>('list_remote_hosts')
  } catch {
    return
  }
  await Promise.allSettled(hosts
    .filter(host => host.enabled && !progress[host.alias])
    .map(host => syncHost(host.alias)))
}

/** 启动定时索引：立即一轮，之后应用在前台（窗口可见）时每 60 秒一轮 */
function startScheduler() {
  if (timer !== null) return
  void init().then(syncAll)
  timer = window.setInterval(() => {
    if (document.visibilityState === 'visible') void syncAll()
  }, INDEX_INTERVAL_MS)
}

export function useRemoteSync() {
  return {
    progress,
    syncing: computed(() => Object.values(progress)),
    syncHost,
    syncAll,
    cancelSync,
    startScheduler,
  }
}
