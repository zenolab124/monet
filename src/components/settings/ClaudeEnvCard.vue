<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

interface ClaudeEnvInfo {
  installedVersion: string | null
  latestVersion: string | null
  updateAvailable: boolean
  binaryPath: string | null
  installMethod: string
}

interface UpgradeResult {
  success: boolean
  newVersion: string | null
  command: string
  outputTail: string
}

interface DiagEntry {
  path: string
  version: string | null
  method: string
  isDefault: boolean
}

const info = ref<ClaudeEnvInfo | null>(null)
const checking = ref(false)
const upgrading = ref(false)
const upgradeMsg = ref<{ kind: 'ok' | 'warn' | 'err'; text: string } | null>(null)
const diag = ref<{ entries: DiagEntry[]; multiple: boolean } | null>(null)
const diagging = ref(false)

const METHOD_KEYS: Record<string, string> = {
  official: 'settings.claudeEnv.methodOfficial',
  npm: 'settings.claudeEnv.methodNpm',
  homebrew: 'settings.claudeEnv.methodHomebrew',
  unknown: 'settings.claudeEnv.methodUnknown',
}

async function check() {
  checking.value = true
  try {
    info.value = await invoke<ClaudeEnvInfo>('claude_env_check')
  } catch { /* ignore */ }
  finally { checking.value = false }
}

async function upgrade() {
  if (upgrading.value) return
  upgrading.value = true
  upgradeMsg.value = null
  try {
    const r = await invoke<UpgradeResult>('claude_env_upgrade')
    const prev = info.value?.installedVersion
    if (r.success && r.newVersion && prev && r.newVersion !== prev) {
      upgradeMsg.value = { kind: 'ok', text: t('settings.claudeEnv.upgradeOk', { version: r.newVersion }) }
    } else if (r.success) {
      upgradeMsg.value = { kind: 'warn', text: t('settings.claudeEnv.upgradeUnchanged') }
    } else {
      upgradeMsg.value = { kind: 'err', text: `${t('settings.claudeEnv.upgradeFail')}\n$ ${r.command}\n${r.outputTail}` }
    }
    await check()
  } catch (e) {
    upgradeMsg.value = { kind: 'err', text: String(e) }
  } finally { upgrading.value = false }
}

async function diagnose() {
  if (diag.value) {
    diag.value = null
    return
  }
  diagging.value = true
  try {
    diag.value = await invoke<{ entries: DiagEntry[]; multiple: boolean }>('claude_env_diagnose')
  } catch { /* ignore */ }
  finally { diagging.value = false }
}

onMounted(check)
</script>

<template>
  <div class="env-card">
    <div class="flex items-center gap-2">
      <span class="i-carbon-cloud-download w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-[11.5px] font-medium">{{ t('settings.claudeEnv.title') }}</span>

      <span v-if="checking" class="text-[10px] text-muted-foreground">{{ t('common.loading') }}</span>
      <template v-else-if="info">
        <span v-if="!info.installedVersion" class="env-badge bad">{{ t('settings.claudeEnv.notFound') }}</span>
        <span v-else-if="info.updateAvailable" class="env-badge warn">{{ t('settings.claudeEnv.updateAvailable') }}</span>
        <span v-else class="env-badge ok">{{ t('settings.claudeEnv.upToDate') }}</span>
        <span v-if="info.installedVersion" class="env-badge off">{{ t(METHOD_KEYS[info.installMethod] ?? METHOD_KEYS.unknown) }}</span>
      </template>

      <span class="flex-1" />
      <button class="env-btn" :disabled="checking || upgrading" @click="check">
        {{ t('settings.claudeEnv.refresh') }}
      </button>
      <button class="env-btn" :disabled="diagging" @click="diagnose">
        {{ diag ? t('settings.claudeEnv.diagHide') : t('settings.claudeEnv.diagnose') }}
      </button>
    </div>

    <!-- 版本对比行:可升级时橙色高亮目标版本 -->
    <div v-if="info?.installedVersion" class="mt-1.5 flex items-baseline gap-1.5 font-mono text-[13px]">
      <span class="text-foreground">{{ info.installedVersion }}</span>
      <template v-if="info.updateAvailable && info.latestVersion">
        <span class="text-muted-foreground">→</span>
        <span class="text-accent font-semibold">{{ info.latestVersion }}</span>
      </template>
      <button
        v-if="info.updateAvailable"
        class="env-btn primary ml-2 flex items-center gap-1"
        :disabled="upgrading"
        @click="upgrade"
      >
        <span v-if="upgrading" class="i-carbon-circle-dash w-3 h-3 animate-spin shrink-0" />
        {{ upgrading ? t('settings.claudeEnv.upgrading') : t('settings.claudeEnv.upgrade') }}
      </button>
    </div>
    <p v-if="info?.binaryPath" class="env-path" :title="info.binaryPath">{{ info.binaryPath }}</p>
    <p v-if="info && !info.latestVersion" class="text-[10px] text-muted-foreground mt-0.5">
      {{ t('settings.claudeEnv.latestUnknown') }}
    </p>

    <!-- 升级结果 -->
    <p
      v-if="upgradeMsg"
      class="mt-1.5 text-[10.5px] whitespace-pre-wrap break-all"
      :class="upgradeMsg.kind === 'ok' ? 'text-primary' : upgradeMsg.kind === 'warn' ? 'text-accent' : 'text-destructive'"
    >{{ upgradeMsg.text }}</p>

    <!-- 冲突诊断 -->
    <div v-if="diag" class="mt-2 border-t border-border pt-1.5">
      <p v-if="diag.multiple" class="text-[10.5px] text-accent mb-1">
        {{ t('settings.claudeEnv.multiWarning', { count: diag.entries.length }) }}
      </p>
      <p v-else class="text-[10.5px] text-muted-foreground mb-1">{{ t('settings.claudeEnv.singleOk') }}</p>
      <div v-for="e in diag.entries" :key="e.path" class="flex items-center gap-1.5 py-0.5">
        <span class="font-mono text-[10px] text-muted-foreground truncate flex-1" :title="e.path">{{ e.path }}</span>
        <span class="font-mono text-[10px] shrink-0">{{ e.version ?? t('settings.claudeEnv.verUnknown') }}</span>
        <span class="env-badge off shrink-0">{{ t(METHOD_KEYS[e.method] ?? METHOD_KEYS.unknown) }}</span>
        <span v-if="e.isDefault" class="env-badge ok shrink-0">{{ t('settings.claudeEnv.default') }}</span>
      </div>
    </div>
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
.env-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  line-height: 1.5;
  white-space: nowrap;
}
.env-badge.ok {
  background: color-mix(in srgb, var(--primary) 12%, transparent);
  color: var(--primary);
}
.env-badge.warn {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: var(--accent);
}
.env-badge.bad {
  background: color-mix(in srgb, var(--destructive) 12%, transparent);
  color: var(--destructive);
}
.env-badge.off {
  background: color-mix(in srgb, var(--muted-foreground) 12%, transparent);
  color: var(--muted-foreground);
}
.env-path {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
  color: var(--muted-foreground);
  margin-top: 2px;
  word-break: break-all;
}
.env-btn {
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 10px;
  font-size: 11px;
  background: var(--background);
  color: var(--foreground);
  cursor: pointer;
  white-space: nowrap;
}
.env-btn.primary {
  background: var(--primary);
  border-color: var(--primary);
  color: var(--primary-foreground);
}
.env-btn:hover:not(:disabled) {
  filter: brightness(1.05);
}
.env-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
