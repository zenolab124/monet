<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

interface RemoteHost {
  alias: string
  instanceId: string
  source: 'ssh_config' | 'manual'
  enabled: boolean
  claudePath: string | null
}

type CheckState = 'pass' | 'warn' | 'fail' | 'skipped'
interface CheckItem { id: string; state: CheckState; detail: string | null }
interface HealthReport {
  items: CheckItem[]
  failure: string | null
  stderrTail: string | null
  cliVersion: string | null
  dataRoot: string | null
  hasProjects: boolean
  usable: boolean
}

const hosts = ref<RemoteHost[]>([])
const reports = reactive<Record<string, HealthReport>>({})
const checking = reactive<Record<string, boolean>>({})
const pathDrafts = reactive<Record<string, string>>({})
const rowErrors = reactive<Record<string, string>>({})
const newConnection = ref('')
const addError = ref<string | null>(null)

const STATE_ICON: Record<CheckState, string> = {
  pass: 'i-carbon-checkmark-filled text-primary',
  warn: 'i-carbon-warning-alt-filled text-accent',
  fail: 'i-carbon-close-filled text-destructive',
  skipped: 'i-carbon-subtract text-muted-foreground',
}

/** 后端错误码 → 文案；未知错误原样显示 */
function errorText(error: unknown): string {
  const code = String(error)
  const key = `remote.error.${code}`
  const text = t(key)
  return text === key ? code : text
}

/** 用户填写的路径无效时给出不同于“自动定位失败”的提示 */
function failureKey(report: HealthReport): string {
  const userPathInvalid = report.items.some(item => item.id === 'cli_path' && item.detail === 'user')
  return report.failure === 'cli_not_found' && userPathInvalid
    ? 'remote.failure.cli_path_invalid'
    : `remote.failure.${report.failure}`
}

async function loadHosts() {
  hosts.value = await invoke<RemoteHost[]>('list_remote_hosts')
  for (const host of hosts.value) pathDrafts[host.alias] = host.claudePath ?? ''
}

async function runCheck(alias: string) {
  checking[alias] = true
  delete rowErrors[alias]
  try {
    reports[alias] = await invoke<HealthReport>('check_remote_host', { alias })
  } catch (error) {
    rowErrors[alias] = errorText(error)
  } finally {
    checking[alias] = false
  }
}

async function saveHost(host: RemoteHost, enabled: boolean) {
  delete rowErrors[host.alias]
  try {
    await invoke('update_remote_host', {
      alias: host.alias,
      enabled,
      claudePath: pathDrafts[host.alias] || null,
    })
    host.enabled = enabled
    host.claudePath = pathDrafts[host.alias] || null
    return true
  } catch (error) {
    rowErrors[host.alias] = errorText(error)
    return false
  }
}

async function toggleHost(host: RemoteHost) {
  const enabling = !host.enabled
  if (await saveHost(host, enabling) && enabling) await runCheck(host.alias)
}

async function savePath(host: RemoteHost) {
  if ((pathDrafts[host.alias] || null) === host.claudePath) return
  if (await saveHost(host, host.enabled) && host.enabled) await runCheck(host.alias)
}

async function addHost() {
  addError.value = null
  try {
    await invoke('add_remote_host', { connection: newConnection.value })
    newConnection.value = ''
    await loadHosts()
  } catch (error) {
    addError.value = errorText(error)
  }
}

async function removeHost(alias: string) {
  try {
    await invoke('remove_remote_host', { alias })
    delete reports[alias]
    await loadHosts()
  } catch (error) {
    rowErrors[alias] = errorText(error)
  }
}

onMounted(async () => {
  try {
    await loadHosts()
  } catch { /* 主机列表不可用时只显示手动添加 */ }
  for (const host of hosts.value) if (host.enabled) void runCheck(host.alias)
})
</script>

<template>
  <div class="env-card">
    <div class="flex items-center gap-2">
      <span class="i-carbon-cloud-satellite w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-[11.5px] font-medium">{{ t('remote.title') }}</span>
    </div>
    <p class="text-[10.5px] text-muted-foreground mt-1 leading-snug">{{ t('remote.intro') }}</p>

    <p v-if="!hosts.length" class="text-[10.5px] text-muted-foreground/80 mt-2">{{ t('remote.empty') }}</p>

    <ul class="mt-2 space-y-2">
      <li v-for="host in hosts" :key="host.alias" class="host-row">
        <div class="flex items-center gap-2">
          <span class="font-mono text-[11px] truncate">{{ host.alias }}</span>
          <span class="host-badge">{{ t(host.source === 'manual' ? 'remote.sourceManual' : 'remote.sourceConfig') }}</span>
          <span class="flex-1" />
          <button
            v-if="host.enabled"
            type="button"
            class="host-action"
            :disabled="checking[host.alias]"
            @click="runCheck(host.alias)"
          >{{ checking[host.alias] ? t('remote.checking') : t('remote.recheck') }}</button>
          <button
            v-if="host.source === 'manual'"
            type="button"
            class="host-action"
            @click="removeHost(host.alias)"
          >{{ t('remote.remove') }}</button>
          <button
            type="button"
            :class="['form-toggle-sm', { on: host.enabled }]"
            :aria-label="t('remote.enable')"
            :aria-pressed="host.enabled"
            @click="toggleHost(host)"
          ><span class="form-toggle-knob" /></button>
        </div>

        <template v-if="host.enabled">
          <label class="mt-1.5 flex items-center gap-2 text-[10.5px] text-muted-foreground">
            <span class="shrink-0">{{ t('remote.claudePath') }}</span>
            <input
              v-model="pathDrafts[host.alias]"
              type="text"
              class="form-input font-mono flex-1 min-w-0 text-[10.5px]"
              :placeholder="t('remote.claudePathPlaceholder')"
              @blur="savePath(host)"
              @keydown.enter="savePath(host)"
            >
          </label>

          <ul v-if="reports[host.alias]" class="mt-1.5 space-y-0.5">
            <li v-for="check in reports[host.alias].items" :key="check.id" class="flex items-start gap-1.5 text-[10.5px]">
              <span :class="[STATE_ICON[check.state], 'w-3 h-3 mt-0.5 shrink-0']" aria-hidden="true" />
              <span class="shrink-0">{{ t(`remote.check.${check.id}`) }}</span>
              <span v-if="check.id === 'cli_path' && check.state === 'fail'" class="text-muted-foreground">{{ t(check.detail === 'user' ? 'remote.cliMissingUser' : 'remote.cliMissingAuto') }}</span>
              <span v-else-if="check.detail && check.id !== 'ssh'" class="env-path truncate">{{ check.detail }}</span>
            </li>
          </ul>
          <p v-if="reports[host.alias]?.failure" class="mt-1 text-[10.5px] text-destructive leading-snug">
            {{ t(failureKey(reports[host.alias]), { alias: host.alias }) }}
          </p>
          <p v-else-if="reports[host.alias]?.items.some(item => item.state === 'warn')" class="mt-1 text-[10.5px] text-accent leading-snug">
            {{ t('remote.cliTooOld') }}
          </p>
          <p v-else-if="reports[host.alias]?.usable && !reports[host.alias]?.hasProjects" class="mt-1 text-[10.5px] text-muted-foreground leading-snug">
            {{ t('remote.noSessions') }}
          </p>
          <pre v-if="reports[host.alias]?.stderrTail" class="stderr-tail">{{ reports[host.alias].stderrTail }}</pre>
        </template>
        <p v-if="rowErrors[host.alias]" class="mt-1 text-[10.5px] text-destructive">{{ rowErrors[host.alias] }}</p>
      </li>
    </ul>

    <form class="mt-2 flex items-center gap-2" @submit.prevent="addHost">
      <input
        v-model="newConnection"
        type="text"
        class="form-input font-mono flex-1 min-w-0 text-[10.5px]"
        :placeholder="t('remote.addPlaceholder')"
      >
      <button type="submit" class="host-action" :disabled="!newConnection.trim()">{{ t('remote.add') }}</button>
    </form>
    <p v-if="addError" class="mt-1 text-[10.5px] text-destructive">{{ addError }}</p>
  </div>
</template>

<style scoped>
.env-card {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  margin-bottom: 8px;
  background: var(--card);
}
.host-row {
  border-top: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
  padding-top: 6px;
}
.host-badge {
  font-size: 9.5px;
  padding: 0 4px;
  border-radius: 3px;
  border: 1px solid var(--border);
  color: var(--muted-foreground);
  white-space: nowrap;
}
.host-action {
  font-size: 11px;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--muted-foreground);
  white-space: nowrap;
}
.host-action:hover:not(:disabled) {
  color: var(--foreground);
}
.host-action:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.env-path {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
  color: var(--muted-foreground);
}
.stderr-tail {
  margin-top: 4px;
  max-height: 72px;
  overflow: auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  color: var(--muted-foreground);
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
