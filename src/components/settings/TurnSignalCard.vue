<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

interface TurnSignalStatus {
  installed: boolean
  healthy: boolean
  issues: string[]
  scriptPath: string
  settingsPath: string
}

const status = ref<TurnSignalStatus | null>(null)
const loading = ref(false)
const error = ref('')

/** 三态：ok（完整可用）/ broken（有痕迹但不完整）/ off（未安装） */
const state = computed<'ok' | 'broken' | 'off'>(() => {
  if (!status.value) return 'off'
  if (status.value.healthy) return 'ok'
  return status.value.installed ? 'broken' : 'off'
})

async function refresh() {
  try {
    status.value = await invoke<TurnSignalStatus>('turn_signal_status')
  } catch { /* ignore */ }
}

async function install() {
  error.value = ''
  loading.value = true
  try {
    status.value = await invoke<TurnSignalStatus>('turn_signal_install')
  } catch (e) {
    error.value = String(e)
  } finally { loading.value = false }
}

async function uninstall() {
  error.value = ''
  loading.value = true
  try {
    status.value = await invoke<TurnSignalStatus>('turn_signal_uninstall')
  } catch (e) {
    error.value = String(e)
  } finally { loading.value = false }
}

onMounted(refresh)
</script>

<template>
  <div class="ext-card">
    <div class="flex items-center gap-2">
      <span class="i-carbon-activity w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-[11.5px] font-medium">{{ t('settings.turnSignal.title') }}</span>
      <span v-if="state === 'ok'" class="ext-badge ok">{{ t('settings.turnSignal.statusOk') }}</span>
      <span v-else-if="state === 'broken'" class="ext-badge bad">{{ t('settings.turnSignal.statusBroken') }}</span>
      <span v-else class="ext-badge off">{{ t('settings.turnSignal.statusOff') }}</span>
    </div>
    <div class="ext-tags">
      <span class="ext-tag neutral">{{ t('settings.extTagAsNeeded') }}</span>
    </div>

    <p class="text-[10.5px] text-muted-foreground mt-1 leading-snug">
      {{ t('settings.turnSignal.desc') }}
    </p>

    <!-- 异常明细 -->
    <ul v-if="state === 'broken' && status?.issues.length" class="mt-1.5 space-y-0.5">
      <li v-for="issue in status.issues" :key="issue" class="text-[10.5px] text-destructive">
        {{ issue }}
      </li>
    </ul>

    <!-- 写入透明化：装了什么、写在哪、如何还原 -->
    <details class="mt-1.5">
      <summary class="ext-writes-summary">{{ t('settings.turnSignal.writesTitle') }}</summary>
      <ul class="mt-1 space-y-1">
        <li class="ext-writes-item">
          {{ t('settings.turnSignal.writes1') }}
          <span class="ext-path">{{ status?.settingsPath || '~/.claude/settings.json' }}</span>
        </li>
        <li class="ext-writes-item">
          {{ t('settings.turnSignal.writes2') }}
          <span class="ext-path">{{ status?.scriptPath || '~/.monet/hooks/turn-signal.sh' }}</span>
        </li>
        <li class="ext-writes-item">{{ t('settings.turnSignal.writes3') }}</li>
      </ul>
    </details>

    <div class="flex gap-1.5 items-center mt-2">
      <button v-if="state === 'off'" class="ext-btn primary" :disabled="loading" @click="install">
        {{ loading ? t('common.loading') : t('settings.turnSignal.install') }}
      </button>
      <template v-else>
        <button v-if="state === 'broken'" class="ext-btn primary" :disabled="loading" @click="install">
          {{ t('settings.turnSignal.reinstall') }}
        </button>
        <button class="ext-btn" :disabled="loading" @click="uninstall">
          {{ t('settings.turnSignal.uninstall') }}
        </button>
      </template>
    </div>

    <p v-if="error" class="text-[10.5px] text-destructive mt-1 break-all">{{ error }}</p>
  </div>
</template>

<style scoped>
.ext-card {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  margin-bottom: 8px;
  background: var(--card);
}
.ext-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  line-height: 1.5;
}
.ext-badge.ok {
  background: color-mix(in srgb, var(--primary) 12%, transparent);
  color: var(--primary);
}
.ext-badge.bad {
  background: color-mix(in srgb, var(--destructive) 12%, transparent);
  color: var(--destructive);
}
.ext-badge.off {
  background: color-mix(in srgb, var(--muted-foreground) 12%, transparent);
  color: var(--muted-foreground);
}
.ext-writes-summary {
  font-size: 10.5px;
  color: var(--muted-foreground);
  cursor: pointer;
  user-select: none;
}
.ext-writes-item {
  font-size: 10.5px;
  color: var(--muted-foreground);
  line-height: 1.5;
}
.ext-path {
  display: block;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  word-break: break-all;
  opacity: 0.85;
}
.ext-btn {
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 10px;
  font-size: 11px;
  background: var(--background);
  color: var(--foreground);
  cursor: pointer;
  white-space: nowrap;
}
.ext-btn.primary {
  background: var(--primary);
  border-color: var(--primary);
  color: var(--primary-foreground);
}
.ext-btn:hover:not(:disabled) {
  filter: brightness(1.05);
}
.ext-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.ext-tags {
  display: flex;
  gap: 4px;
  margin-top: 5px;
}
.ext-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 3px;
  line-height: 1.5;
}
.ext-tag.neutral {
  background: color-mix(in srgb, var(--muted-foreground) 10%, transparent);
  color: var(--muted-foreground);
}
</style>
