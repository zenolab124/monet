<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useProjects } from '@/composables/useProjects'

const { t } = useI18n()
const { projects } = useProjects()

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
  success: []
}>()

// 表单状态
const name = ref('')
const scope = ref<'user' | 'local' | 'project'>('local')
const projectCwd = ref('')
const transport = ref<'stdio' | 'http' | 'sse'>('stdio')
const commandOrUrl = ref('')
const args = ref('')
const env = ref('')
const submitting = ref(false)
const errorMsg = ref('')

// 校验错误
const nameError = ref('')
const commandError = ref('')
const projectError = ref('')
const envError = ref('')

// 项目下拉选项：取路径最后一段做标签
const projectOptions = computed(() =>
  projects.value.map(p => ({
    value: p.display_path,
    label: p.display_path.split('/').filter(Boolean).pop() || p.display_path,
  })),
)

// 命令/URL 标签随 transport 变化
const commandLabel = computed(() =>
  transport.value === 'stdio'
    ? t('workshop.mcpAddCommand')
    : t('workshop.mcpAddUrl'),
)

// 重置表单（打开时清空）
watch(() => props.visible, (v) => {
  if (v) {
    name.value = ''
    scope.value = 'local'
    projectCwd.value = ''
    transport.value = 'stdio'
    commandOrUrl.value = ''
    args.value = ''
    env.value = ''
    submitting.value = false
    errorMsg.value = ''
    nameError.value = ''
    commandError.value = ''
    projectError.value = ''
    envError.value = ''
  }
})

/** 解析环境变量，返回 null 表示格式错误 */
function parseEnv(raw: string): Record<string, string> | null {
  const result: Record<string, string> = {}
  const lines = raw.split('\n').filter(l => l.trim())
  for (const line of lines) {
    const idx = line.indexOf('=')
    if (idx <= 0) return null
    const key = line.slice(0, idx).trim()
    const val = line.slice(idx + 1)
    if (!key) return null
    result[key] = val
  }
  return result
}

/** 提交 */
async function handleSubmit() {
  // 清除旧错误
  nameError.value = ''
  commandError.value = ''
  projectError.value = ''
  envError.value = ''
  errorMsg.value = ''

  // 校验
  let valid = true
  if (!name.value.trim()) {
    nameError.value = t('workshop.mcpAddNameRequired')
    valid = false
  }
  if (!commandOrUrl.value.trim()) {
    commandError.value = t('workshop.mcpAddCommandRequired')
    valid = false
  }
  if (scope.value !== 'user' && !projectCwd.value) {
    projectError.value = t('workshop.mcpAddProjectRequired')
    valid = false
  }

  // 解析 env
  let parsedEnv: Record<string, string> = {}
  if (env.value.trim()) {
    const result = parseEnv(env.value)
    if (result === null) {
      envError.value = t('workshop.mcpAddEnvInvalid')
      valid = false
    } else {
      parsedEnv = result
    }
  }

  if (!valid) return

  // 解析 args（按空格拆分，过滤空串）
  const parsedArgs = transport.value === 'stdio'
    ? args.value.split(/\s+/).filter(Boolean)
    : []

  submitting.value = true
  try {
    await invoke('mcp_add', {
      name: name.value.trim(),
      scope: scope.value,
      transport: transport.value,
      commandOrUrl: commandOrUrl.value.trim(),
      args: parsedArgs,
      env: parsedEnv,
      projectCwd: scope.value !== 'user' ? projectCwd.value : '',
    })
    emit('success')
    emit('close')
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <div
    v-if="visible"
    class="fixed inset-0 z-70 grid place-items-center"
    style="background: rgba(70, 45, 20, 0.18)"
    @mousedown.self="emit('close')"
  >
    <div class="dialog-card">
      <!-- 标题 -->
      <h3 class="text-sm font-600 text-foreground mb-3">
        {{ t('workshop.mcpAddTitle') }}
      </h3>

      <form @submit.prevent="handleSubmit" class="flex flex-col gap-3">
        <!-- 名称 -->
        <div class="field">
          <label class="field-label">{{ t('workshop.mcpAddName') }}</label>
          <input
            v-model="name"
            type="text"
            :placeholder="t('workshop.mcpAddNamePlaceholder')"
            class="field-input"
          >
          <span v-if="nameError" class="field-error">{{ nameError }}</span>
        </div>

        <!-- 作用域 -->
        <div class="field">
          <label class="field-label">{{ t('workshop.mcpAddScope') }}</label>
          <div class="flex flex-wrap gap-3">
            <label class="radio-label">
              <input v-model="scope" type="radio" value="user" class="radio-input">
              <span>{{ t('workshop.mcpScopeUser') }}</span>
            </label>
            <label class="radio-label">
              <input v-model="scope" type="radio" value="local" class="radio-input">
              <span>{{ t('workshop.mcpScopeLocal') }}</span>
            </label>
            <label class="radio-label">
              <input v-model="scope" type="radio" value="project" class="radio-input">
              <span>{{ t('workshop.mcpScopeProject') }}</span>
            </label>
          </div>
        </div>

        <!-- 项目选择（scope != user 时显示） -->
        <div v-if="scope !== 'user'" class="field">
          <label class="field-label">{{ t('workshop.mcpAddProject') }}</label>
          <select v-model="projectCwd" class="field-input">
            <option value="" disabled>--</option>
            <option
              v-for="opt in projectOptions"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </option>
          </select>
          <span v-if="projectError" class="field-error">{{ projectError }}</span>
        </div>

        <!-- 传输方式 -->
        <div class="field">
          <label class="field-label">{{ t('workshop.mcpAddTransport') }}</label>
          <div class="flex gap-3">
            <label class="radio-label">
              <input v-model="transport" type="radio" value="stdio" class="radio-input">
              <span>stdio</span>
            </label>
            <label class="radio-label">
              <input v-model="transport" type="radio" value="http" class="radio-input">
              <span>http</span>
            </label>
            <label class="radio-label">
              <input v-model="transport" type="radio" value="sse" class="radio-input">
              <span>sse</span>
            </label>
          </div>
        </div>

        <!-- 命令 / URL -->
        <div class="field">
          <label class="field-label">{{ commandLabel }}</label>
          <input
            v-model="commandOrUrl"
            type="text"
            class="field-input"
          >
          <span v-if="commandError" class="field-error">{{ commandError }}</span>
        </div>

        <!-- Args（仅 stdio） -->
        <div v-if="transport === 'stdio'" class="field">
          <label class="field-label">{{ t('workshop.mcpAddArgs') }}</label>
          <input
            v-model="args"
            type="text"
            :placeholder="t('workshop.mcpAddArgsPlaceholder')"
            class="field-input"
          >
        </div>

        <!-- 环境变量 -->
        <div class="field">
          <label class="field-label">{{ t('workshop.mcpAddEnv') }}</label>
          <textarea
            v-model="env"
            :placeholder="t('workshop.mcpAddEnvPlaceholder')"
            class="field-input field-textarea"
            rows="3"
          />
          <span class="text-[10.5px] text-muted-foreground mt-0.5">
            {{ t('workshop.mcpAddEnvHint') }}
          </span>
          <span v-if="envError" class="field-error">{{ envError }}</span>
        </div>

        <!-- 错误信息 -->
        <div v-if="errorMsg" class="error-box">
          <span class="font-500">{{ t('workshop.mcpAddFailed') }}</span>
          <pre class="whitespace-pre-wrap text-[10.5px] mt-1">{{ errorMsg }}</pre>
        </div>

        <!-- 按钮行 -->
        <div class="flex justify-end gap-2 mt-1">
          <button type="button" class="btn btn-cancel" @click="emit('close')">
            {{ t('common.cancel') }}
          </button>
          <button type="submit" class="btn btn-primary" :disabled="submitting">
            <span v-if="submitting" class="i-carbon-circle-dash animate-spin w-3.5 h-3.5 mr-1" />
            {{ t('workshop.mcpAddSubmit') }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.dialog-card {
  width: 100%;
  max-width: 480px;
  border-radius: 6px;
  background: var(--popover);
  border: 1px solid var(--border);
  box-shadow: var(--shadow-paper-lifted);
  padding: 16px 20px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.field-label {
  font-size: 11.5px;
  font-weight: 600;
  color: var(--muted-foreground);
}

.field-input {
  font-size: 12.5px;
  padding: 5px 8px;
  border-radius: 4px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--foreground);
  outline: none;
  transition: border-color 0.15s, box-shadow 0.15s;
}
.field-input:focus {
  border-color: var(--primary);
  box-shadow: 0 0 0 2px oklch(from var(--primary) l c h / 0.12);
}

.field-textarea {
  resize: vertical;
  min-height: 56px;
  font-family: var(--font-mono, ui-monospace, monospace);
}

.field-error {
  font-size: 11px;
  color: var(--destructive);
}

.radio-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--foreground);
  cursor: pointer;
}

.radio-input {
  accent-color: var(--primary);
  width: 13px;
  height: 13px;
  margin: 0;
}

.error-box {
  font-size: 11.5px;
  color: var(--destructive);
  background: oklch(from var(--destructive) l c h / 0.06);
  border: 1px solid oklch(from var(--destructive) l c h / 0.2);
  border-radius: 4px;
  padding: 8px 10px;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 5px 12px;
  font-size: 12px;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.15s;
}
.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-cancel {
  border: 1px solid var(--border);
  color: var(--muted-foreground);
  background: transparent;
}
.btn-cancel:hover {
  color: var(--foreground);
  background: var(--muted);
}

.btn-primary {
  border: none;
  background: var(--primary);
  color: var(--primary-foreground);
}
.btn-primary:hover:not(:disabled) {
  box-shadow: var(--shadow-paper);
}
</style>
