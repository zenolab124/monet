<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

interface LocateInfo {
  path: string | null
  source: 'manual' | 'cached' | 'scan' | 'loginShell' | null
  manualPath: string | null
  manualValid: boolean
  attempted: string[]
}

const info = ref<LocateInfo | null>(null)
const loading = ref(false)
const manualInput = ref('')
const error = ref('')

function applyInfo(next: LocateInfo) {
  info.value = next
  manualInput.value = next.manualPath ?? ''
}

async function load() {
  loading.value = true
  try {
    applyInfo(await invoke<LocateInfo>('get_claude_binary_info'))
  } catch { /* ignore */ }
  finally { loading.value = false }
}

async function saveManual() {
  error.value = ''
  loading.value = true
  try {
    applyInfo(await invoke<LocateInfo>('set_claude_binary_path', {
      path: manualInput.value.trim() || null,
    }))
  } catch (e) {
    error.value = String(e)
  } finally { loading.value = false }
}

async function clearManual() {
  manualInput.value = ''
  await saveManual()
}

async function redetect() {
  error.value = ''
  loading.value = true
  try {
    applyInfo(await invoke<LocateInfo>('redetect_claude_binary'))
  } catch { /* ignore */ }
  finally { loading.value = false }
}

const SOURCE_KEYS: Record<string, string> = {
  manual: 'settings.claudeBin.sourceManual',
  cached: 'settings.claudeBin.sourceCached',
  scan: 'settings.claudeBin.sourceScan',
  loginShell: 'settings.claudeBin.sourceLoginShell',
}

onMounted(load)
</script>

<template>
  <div class="bin-card">
    <div class="flex items-center gap-2">
      <span class="i-carbon-terminal w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-[11.5px] font-medium">{{ t('settings.claudeBin.title') }}</span>
      <span v-if="loading" class="text-[10px] text-muted-foreground">{{ t('common.loading') }}</span>
      <template v-else-if="info">
        <span v-if="info.path" class="bin-badge ok">
          {{ t(SOURCE_KEYS[info.source ?? ''] ?? '') }}
        </span>
        <span v-else class="bin-badge bad">{{ t('settings.claudeBin.notFound') }}</span>
      </template>
    </div>

    <p v-if="info?.path" class="bin-path" :title="info.path">{{ info.path }}</p>

    <p class="text-[10.5px] text-muted-foreground mt-1 leading-snug">
      {{ t('settings.claudeBin.description') }}
    </p>

    <!-- 手动路径无效提示 -->
    <p v-if="info?.manualPath && !info.manualValid" class="text-[10.5px] text-destructive mt-1">
      {{ t('settings.claudeBin.manualInvalid') }}
    </p>

    <!-- 探测失败：展示尝试清单 -->
    <div v-if="info && !info.path" class="mt-1.5">
      <p class="text-[10.5px] text-destructive">{{ t('settings.claudeBin.attemptedTitle') }}</p>
      <ul class="mt-0.5 space-y-0.5">
        <li v-for="a in info.attempted" :key="a" class="bin-path !mt-0">{{ a }}</li>
      </ul>
    </div>

    <div class="flex gap-1.5 items-center mt-2">
      <input
        v-model="manualInput"
        class="bin-input flex-1"
        :placeholder="t('settings.claudeBin.manualPlaceholder')"
        spellcheck="false"
        @keydown.enter="saveManual"
      >
      <button class="bin-btn" :disabled="loading" @click="saveManual">
        {{ t('common.save') }}
      </button>
      <button
        v-if="info?.manualPath"
        class="bin-btn"
        :disabled="loading"
        @click="clearManual"
      >
        {{ t('common.clear') }}
      </button>
      <button class="bin-btn" :disabled="loading" @click="redetect">
        {{ t('settings.claudeBin.redetect') }}
      </button>
    </div>

    <p v-if="error" class="text-[10.5px] text-destructive mt-1">{{ error }}</p>
  </div>
</template>

<style scoped>
.bin-card {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  margin-bottom: 8px;
  background: var(--card);
}
.bin-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  line-height: 1.5;
}
.bin-badge.ok {
  background: color-mix(in srgb, var(--primary) 12%, transparent);
  color: var(--primary);
}
.bin-badge.bad {
  background: color-mix(in srgb, var(--destructive) 12%, transparent);
  color: var(--destructive);
}
.bin-path {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
  color: var(--muted-foreground);
  margin-top: 4px;
  word-break: break-all;
}
.bin-input {
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 8px;
  font-size: 11px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  background: var(--background);
  color: var(--foreground);
  min-width: 0;
}
.bin-input:focus {
  outline: none;
  border-color: var(--ring);
}
.bin-btn {
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 10px;
  font-size: 11px;
  background: var(--background);
  color: var(--foreground);
  cursor: pointer;
  white-space: nowrap;
}
.bin-btn:hover:not(:disabled) {
  background: var(--accent);
}
.bin-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
