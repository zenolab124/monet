<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

// Rust 端 serde(rename_all = "camelCase") 返回的结构
interface ClaudeRootInfo {
  effective: string
  configured: string | null
  source: 'env' | 'settings' | 'default'
  default: string
  exists: boolean
  restartRequired: boolean
}

const rootInfo = ref<ClaudeRootInfo | null>(null)
const loading = ref(false)
const error = ref('')

const SOURCE_KEYS: Record<string, string> = {
  env: 'settings.claudeRoot.sourceEnv',
  settings: 'settings.claudeRoot.sourceSettings',
  default: 'settings.claudeRoot.sourceDefault',
}

// 环境变量控制时禁用编辑
const isEnvControlled = computed(() => rootInfo.value?.source === 'env')

async function loadRoot() {
  loading.value = true
  try {
    rootInfo.value = await invoke<ClaudeRootInfo>('get_claude_root_info')
  } catch { /* ignore */ }
  finally { loading.value = false }
}

async function pickDir() {
  const selected = await open({ directory: true, multiple: false })
  if (!selected) return
  error.value = ''
  loading.value = true
  try {
    rootInfo.value = await invoke<ClaudeRootInfo>('set_claude_root', { path: selected })
  } catch (e) {
    error.value = String(e)
  } finally { loading.value = false }
}

async function resetDefault() {
  error.value = ''
  loading.value = true
  try {
    rootInfo.value = await invoke<ClaudeRootInfo>('set_claude_root', { path: null })
  } catch (e) {
    error.value = String(e)
  } finally { loading.value = false }
}

onMounted(loadRoot)
</script>

<template>
  <div class="env-card">
    <!-- 标题行：图标 + 标题 + 来源 badge -->
    <div class="flex items-center gap-2">
      <span class="i-carbon-folder-shared w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-[11.5px] font-medium">{{ t('settings.claudeRoot.title') }}</span>

      <span v-if="loading" class="text-[10px] text-muted-foreground">{{ t('common.loading') }}</span>
      <template v-else-if="rootInfo">
        <span class="env-badge off">{{ t(SOURCE_KEYS[rootInfo.source] ?? '') }}</span>
        <span v-if="!rootInfo.exists" class="env-badge bad">{{ t('settings.claudeRoot.pathMissing') }}</span>
      </template>
    </div>

    <!-- 说明文案 -->
    <p class="text-[10.5px] text-muted-foreground mt-1 leading-snug">
      {{ t('settings.claudeRoot.description') }}
    </p>

    <!-- 当前生效路径 -->
    <p v-if="rootInfo" class="env-path" :title="rootInfo.effective">{{ rootInfo.effective }}</p>

    <!-- 环境变量控制提示 -->
    <p v-if="isEnvControlled" class="text-[10.5px] text-accent mt-1">
      {{ t('settings.claudeRoot.envControlled') }}
    </p>

    <!-- 重启提示 -->
    <p v-if="rootInfo?.restartRequired" class="text-[10.5px] text-accent mt-1">
      {{ t('settings.claudeRoot.restartHint') }}
    </p>

    <!-- 错误信息 -->
    <p v-if="error" class="text-[10.5px] text-destructive mt-1">{{ error }}</p>

    <!-- 操作按钮行 -->
    <div v-if="!isEnvControlled" class="flex gap-1.5 items-center mt-2">
      <button class="env-btn" :disabled="loading" @click="pickDir">
        {{ t('settings.claudeRoot.pickDir') }}
      </button>
      <button
        v-if="rootInfo?.configured"
        class="env-btn"
        :disabled="loading"
        @click="resetDefault"
      >
        {{ t('settings.claudeRoot.resetDefault') }}
      </button>
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
.env-badge.off {
  background: color-mix(in srgb, var(--muted-foreground) 12%, transparent);
  color: var(--muted-foreground);
}
.env-badge.bad {
  background: color-mix(in srgb, var(--destructive) 12%, transparent);
  color: var(--destructive);
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
.env-btn:hover:not(:disabled) {
  filter: brightness(1.05);
}
.env-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
