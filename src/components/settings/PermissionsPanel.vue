<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification'
import { useNotifications } from '@/composables/useNotifications'

type Status = 'granted' | 'denied' | 'undetermined' | 'targetNotRunning' | 'unknown'

interface PermRow {
  key: string
  icon: string
  /** 深链系统设置的面板锚点；null 表示无对应面板 */
  panel: string | null
  /** 可通过应用内动作触发系统授权弹窗 */
  requestable: boolean
}

const { t } = useI18n()
const { notifyTransient } = useNotifications()

// 主应用账本：前台会话、工作台任务、终端恢复的权限归因都挂在主 app 上
const appRows: PermRow[] = [
  { key: 'automationTerminal', icon: 'i-carbon-terminal', panel: 'automation', requestable: true },
  { key: 'notifications', icon: 'i-carbon-notification', panel: null, requestable: true },
  { key: 'fullDiskAccess', icon: 'i-carbon-data-base', panel: 'allFiles', requestable: false },
  { key: 'accessibility', icon: 'i-carbon-accessibility', panel: 'accessibility', requestable: true },
  { key: 'screenCapture', icon: 'i-carbon-screen', panel: 'screenRecording', requestable: true },
  { key: 'localNetwork', icon: 'i-carbon-network-3', panel: 'localNetwork', requestable: false },
]

// runner 账本：launchd 启动的定时任务（含其中 Claude 用到的能力）归因挂 runner
const runnerRows: PermRow[] = [
  { key: 'automationSystemEvents', icon: 'i-carbon-power', panel: 'automation', requestable: false },
  { key: 'fullDiskAccess', icon: 'i-carbon-data-base', panel: 'allFiles', requestable: false },
  { key: 'accessibility', icon: 'i-carbon-accessibility', panel: 'accessibility', requestable: false },
  { key: 'screenCapture', icon: 'i-carbon-screen', panel: 'screenRecording', requestable: false },
]

const appPerms = ref<Record<string, Status>>({})
const runnerResult = ref<{ checkedAt: string; permissions: Record<string, Status> } | null>(null)
const checking = ref(false)
const runnerChecking = ref(false)
const requesting = ref<string | null>(null)

function appStatus(key: string): Status {
  if (key === 'localNetwork') return 'unknown'
  return appPerms.value[key] ?? 'unknown'
}

function runnerStatus(key: string): Status {
  return runnerResult.value?.permissions[key] ?? 'unknown'
}

const STATUS_DOT: Record<Status, string> = {
  granted: 'bg-green-600',
  denied: 'bg-red-500',
  undetermined: 'bg-amber-500',
  targetNotRunning: 'bg-muted-foreground/50',
  unknown: 'bg-muted-foreground/50',
}

async function refresh() {
  checking.value = true
  try {
    const [perms, notif] = await Promise.all([
      invoke<Record<string, Status>>('check_system_permissions'),
      isPermissionGranted(),
    ])
    appPerms.value = { ...perms, notifications: notif ? 'granted' : 'undetermined' }
  } catch (e) {
    notifyTransient(t('common.loadFailed'), String(e))
  } finally {
    checking.value = false
  }
}

async function requestApp(row: PermRow) {
  requesting.value = row.key
  try {
    if (row.key === 'notifications') {
      const r = await requestPermission()
      appPerms.value = { ...appPerms.value, notifications: r === 'granted' ? 'granted' : 'denied' }
    } else {
      const status = await invoke<Status>('request_system_permission', { kind: row.key })
      appPerms.value = { ...appPerms.value, [row.key]: status }
      // 屏幕录制授权写入后，本进程要重启才能读到新状态
      if (row.key === 'screenCapture' && status !== 'granted') {
        notifyTransient(t('settings.permCheck.screenRestartHint'))
      }
    }
  } catch (e) {
    notifyTransient(t('settings.permCheck.requestFailed'), String(e))
  } finally {
    requesting.value = null
  }
}

// runner 行的授权请求：经 launchd 以 prompt 模式跑一次，系统弹窗归因给 runner
async function requestRunner(row: PermRow) {
  requesting.value = `runner:${row.key}`
  try {
    const before = runnerStatus(row.key)
    runnerResult.value = await invoke('run_runner_health_check', { promptKind: row.key })
    // denied 记录系统不再弹窗，且 runner 是路径型记录无法程序化重置，
    // 只能引导用户去系统设置删除旧条目后重试
    if (runnerStatus(row.key) === 'denied' && before === 'denied') {
      notifyTransient(t('settings.permCheck.stillDenied'), t('settings.permCheck.stillDeniedHint'))
    }
  } catch (e) {
    notifyTransient(t('settings.permCheck.requestFailed'), String(e))
  } finally {
    requesting.value = null
  }
}

function openPanel(panel: string | null) {
  if (panel) invoke('open_privacy_settings', { panel })
}

async function runRunnerCheck() {
  runnerChecking.value = true
  try {
    runnerResult.value = await invoke('run_runner_health_check')
  } catch (e) {
    notifyTransient(t('settings.permCheck.runnerCheckFailed'), String(e))
  } finally {
    runnerChecking.value = false
  }
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  return isNaN(d.getTime()) ? iso : d.toLocaleString()
}

// 用户去系统设置改完权限切回来时自动重新检测
function onWindowFocus() {
  if (!checking.value) refresh()
}

onMounted(async () => {
  refresh()
  window.addEventListener('focus', onWindowFocus)
  try {
    runnerResult.value = await invoke('get_runner_health_snapshot')
  } catch {
    /* 无历史结果 */
  }
})

onUnmounted(() => {
  window.removeEventListener('focus', onWindowFocus)
})
</script>

<template>
  <div class="space-y-4">
    <p class="text-xs text-muted-foreground leading-relaxed">{{ t('settings.permCheck.desc') }}</p>

    <!-- 主应用账本 -->
    <div class="border border-border rounded-md overflow-hidden">
      <div class="flex items-center justify-between px-3 py-2 bg-muted/40 border-b border-border">
        <div class="text-xs font-medium">{{ t('settings.permCheck.appGroup') }}</div>
        <button class="perm-btn" :disabled="checking" @click="refresh">
          <span :class="checking ? 'i-carbon-circle-dash animate-spin' : 'i-carbon-renew'" class="w-3 h-3" />
          {{ t('settings.permCheck.refresh') }}
        </button>
      </div>
      <div
        v-for="row in appRows"
        :key="row.key"
        class="flex items-center gap-2.5 px-3 py-2 border-b border-border last:border-b-0"
      >
        <span :class="row.icon" class="w-4 h-4 shrink-0 opacity-70" />
        <div class="flex-1 min-w-0">
          <div class="text-xs">{{ t(`settings.permCheck.rows.${row.key}`) }}</div>
          <div class="text-[11px] text-muted-foreground truncate">{{ t(`settings.permCheck.rows.${row.key}Desc`) }}</div>
        </div>
        <span class="flex items-center gap-1.5 text-[11px] text-muted-foreground shrink-0">
          <i class="inline-block w-1.5 h-1.5 rounded-full" :class="STATUS_DOT[appStatus(row.key)]" />
          {{ t(`settings.permCheck.status.${appStatus(row.key)}`) }}
        </span>
        <button
          v-if="row.requestable && appStatus(row.key) !== 'granted'"
          class="perm-btn"
          :disabled="requesting === row.key"
          @click="requestApp(row)"
        >
          <span v-if="requesting === row.key" class="i-carbon-circle-dash animate-spin w-3 h-3" />
          {{ t('settings.permCheck.request') }}
        </button>
        <button v-if="row.panel && appStatus(row.key) !== 'granted'" class="perm-btn" @click="openPanel(row.panel)">
          {{ t('settings.permCheck.openSettings') }}
        </button>
      </div>
    </div>

    <!-- runner 账本 -->
    <div class="border border-border rounded-md overflow-hidden">
      <div class="flex items-center justify-between px-3 py-2 bg-muted/40 border-b border-border">
        <div class="min-w-0">
          <div class="text-xs font-medium">{{ t('settings.permCheck.runnerGroup') }}</div>
          <div class="text-[11px] text-muted-foreground">
            {{ runnerResult
              ? t('settings.permCheck.lastChecked', { time: formatTime(runnerResult.checkedAt) })
              : t('settings.permCheck.neverChecked') }}
          </div>
        </div>
        <button class="perm-btn shrink-0" :disabled="runnerChecking" @click="runRunnerCheck">
          <span :class="runnerChecking ? 'i-carbon-circle-dash animate-spin' : 'i-carbon-play'" class="w-3 h-3" />
          {{ runnerChecking ? t('settings.permCheck.checking') : t('settings.permCheck.runCheck') }}
        </button>
      </div>
      <p class="px-3 py-2 text-[11px] text-muted-foreground border-b border-border leading-relaxed">
        {{ t('settings.permCheck.runnerGroupDesc') }}
      </p>
      <div
        v-for="row in runnerRows"
        :key="row.key"
        class="flex items-center gap-2.5 px-3 py-2 border-b border-border last:border-b-0"
      >
        <span :class="row.icon" class="w-4 h-4 shrink-0 opacity-70" />
        <div class="flex-1 min-w-0">
          <div class="text-xs">{{ t(`settings.permCheck.rows.${row.key}`) }}</div>
          <div v-if="row.key === 'automationSystemEvents'" class="text-[11px] text-muted-foreground truncate">
            {{ t('settings.permCheck.rows.automationSystemEventsDesc') }}
          </div>
        </div>
        <span class="flex items-center gap-1.5 text-[11px] text-muted-foreground shrink-0">
          <i class="inline-block w-1.5 h-1.5 rounded-full" :class="STATUS_DOT[runnerStatus(row.key)]" />
          {{ t(`settings.permCheck.status.${runnerStatus(row.key)}`) }}
        </span>
        <button
          v-if="row.key !== 'fullDiskAccess' && runnerStatus(row.key) !== 'granted'"
          class="perm-btn"
          :disabled="requesting === `runner:${row.key}` || runnerChecking"
          @click="requestRunner(row)"
        >
          <span v-if="requesting === `runner:${row.key}`" class="i-carbon-circle-dash animate-spin w-3 h-3" />
          {{ t('settings.permCheck.request') }}
        </button>
        <button v-if="row.panel && runnerStatus(row.key) !== 'granted'" class="perm-btn" @click="openPanel(row.panel)">
          {{ t('settings.permCheck.openSettings') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.perm-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid hsl(var(--border));
  border-radius: 5px;
  background: hsl(var(--card));
  color: hsl(var(--foreground));
  white-space: nowrap;
}
.perm-btn:hover:not(:disabled) {
  background: hsl(var(--muted));
}
.perm-btn:disabled {
  opacity: 0.5;
}
</style>
