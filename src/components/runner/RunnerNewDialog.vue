<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  visible: boolean
  defaultCwd: string
  spawnError?: string
}>()

const emit = defineEmits<{
  close: []
  spawn: [spec: { cmd: string; cwd: string; alias: string; env: Record<string, string> }]
}>()

const { t } = useI18n()

const cmd = ref('')
const cwd = ref('')
const alias = ref('')
const envText = ref('')
const envOpen = ref(false)
const error = ref('')
const cmdInputRef = ref<HTMLInputElement>()

// spawn 失败时由父组件经 prop 传入错误文案
watch(() => props.spawnError, (err) => {
  if (err) error.value = err
})

// 每次打开重置表单
watch(() => props.visible, (v) => {
  if (v) {
    cmd.value = ''
    cwd.value = props.defaultCwd
    alias.value = ''
    envText.value = ''
    envOpen.value = false
    error.value = ''
    nextTick(() => cmdInputRef.value?.focus())
  }
})

/** 解析 KEY=VALUE 格式的环境变量文本 */
function parseEnv(text: string): Record<string, string> {
  const env: Record<string, string> = {}
  for (const line of text.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    const idx = trimmed.indexOf('=')
    if (idx <= 0) continue
    env[trimmed.slice(0, idx).trim()] = trimmed.slice(idx + 1).trim()
  }
  return env
}

function onSubmit() {
  const cmdVal = cmd.value.trim()
  if (!cmdVal) return
  error.value = ''
  emit('spawn', {
    cmd: cmdVal,
    cwd: cwd.value.trim() || props.defaultCwd,
    alias: alias.value.trim(),
    env: parseEnv(envText.value),
  })
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="dialog-backdrop" @mousedown.self="emit('close')">
      <div class="dialog-card" @keydown.escape="emit('close')">
        <div class="dlg-head">{{ t('runner.newRunner') }}</div>
        <div class="dlg-body">
          <div class="field">
            <label>{{ t('runner.commandLabel') }} <span class="text-accent">*</span></label>
            <input
              ref="cmdInputRef"
              v-model="cmd"
              class="field-input font-mono"
              :placeholder="t('runner.commandPlaceholder')"
              @keydown.enter="onSubmit"
            />
          </div>
          <div class="field">
            <label>{{ t('runner.cwdLabel') }}</label>
            <input v-model="cwd" class="field-input font-mono" />
          </div>
          <div class="field">
            <label>{{ t('runner.aliasLabel') }}</label>
            <input
              v-model="alias"
              class="field-input"
              :placeholder="t('runner.aliasPlaceholder')"
            />
          </div>
          <button class="env-fold" @click="envOpen = !envOpen">
            {{ envOpen ? '▾' : '▸' }} {{ t('runner.envOverride') }}
          </button>
          <textarea
            v-if="envOpen"
            v-model="envText"
            class="field-input font-mono env-textarea"
            :placeholder="t('runner.envPlaceholder')"
            rows="3"
          />
          <div v-if="error" class="text-destructive text-[11px] mt-1">{{ error }}</div>
        </div>
        <div class="dlg-foot">
          <button class="btn-cancel" @click="emit('close')">{{ t('common.cancel') }}</button>
          <button class="btn-primary" :disabled="!cmd.trim()" @click="onSubmit">{{ t('runner.launch') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklch, var(--foreground) 6%, transparent);
}
.dialog-card {
  width: 400px;
  background: var(--popover);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: var(--shadow-paper-lifted);
  overflow: hidden;
}
.dlg-head {
  padding: 12px 16px 0;
  font-size: 13px;
  font-weight: 600;
}
.dlg-body {
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.field label {
  display: block;
  font-size: 11px;
  color: var(--muted-foreground);
  margin-bottom: 3px;
}
.field-input {
  width: 100%;
  font-size: 12px;
  padding: 5px 9px;
  border-radius: var(--radius);
  border: 1px solid var(--input);
  background: var(--card);
  color: var(--foreground);
}
.env-fold {
  font-size: 11px;
  color: var(--muted-foreground);
  cursor: pointer;
  user-select: none;
  display: flex;
  align-items: center;
  gap: 4px;
  border: none;
  background: none;
  padding: 0;
}
.env-textarea {
  resize: vertical;
  min-height: 48px;
  line-height: 1.5;
}
.dlg-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 16px 14px;
}
.btn-cancel {
  font-size: 11px;
  padding: 3px 10px;
  border-radius: var(--radius);
  cursor: pointer;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--muted-foreground);
}
.btn-cancel:hover { color: var(--foreground); box-shadow: var(--shadow-paper); }
.btn-primary {
  font-size: 11.5px;
  padding: 4px 14px;
  border-radius: var(--radius);
  cursor: pointer;
  border: none;
  background: var(--primary);
  color: var(--primary-foreground);
}
.btn-primary:hover { box-shadow: var(--shadow-paper); }
.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
