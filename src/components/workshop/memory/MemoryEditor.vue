<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useMemory } from '@/composables/useMemory'

/**
 * 记忆编辑态（v2.9.0 FR-009）：
 * 等宽 textarea 编辑整文件原文，保存前校验 mtime 未被外部修改。
 * 加载用 get_memory_raw（整合者补）获取原始内容。
 */

const { t } = useI18n()
const { selectedProjectDir, selectedFile, saveMemory } = useMemory()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const content = ref('')
const originalMtime = ref(0)
const loading = ref(true)
const saving = ref(false)
const errorMsg = ref<string | null>(null)
const externallyModified = ref(false)

onMounted(async () => {
  if (!selectedProjectDir.value || !selectedFile.value) return
  loading.value = true
  try {
    // 用 get_memory_raw 获取原始文件内容（整合者补充的 command）
    const result = await invoke<{ content: string; mtime: number }>('get_memory_raw', {
      projectDir: selectedProjectDir.value,
      file: selectedFile.value,
    })
    content.value = result.content
    originalMtime.value = result.mtime
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    loading.value = false
  }
})

async function handleSave() {
  saving.value = true
  errorMsg.value = null
  externallyModified.value = false

  const result = await saveMemory(content.value, originalMtime.value)
  saving.value = false

  if (result.ok) {
    emit('close')
  } else if (result.error?.includes('modified_externally')) {
    externallyModified.value = true
  } else {
    errorMsg.value = result.error ?? t('common.loadFailed')
  }
}

async function handleReload() {
  externallyModified.value = false
  errorMsg.value = null
  loading.value = true
  try {
    const result = await invoke<{ content: string; mtime: number }>('get_memory_raw', {
      projectDir: selectedProjectDir.value!,
      file: selectedFile.value!,
    })
    content.value = result.content
    originalMtime.value = result.mtime
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="edit-block">
    <!-- 提示条 -->
    <div class="edit-tip">{{ t('memory.editTip') }}</div>

    <!-- 加载中 -->
    <div v-if="loading" class="edit-loading">{{ t('common.loading') }}</div>

    <!-- 编辑区 -->
    <template v-else>
      <!-- 外部修改警告 -->
      <div v-if="externallyModified" class="edit-warn">
        <span>{{ t('memory.externallyModified') }}</span>
        <button class="ws-btn" @click="handleReload">{{ t('memory.reload') }}</button>
      </div>

      <!-- 错误 -->
      <div v-if="errorMsg && !externallyModified" class="edit-error">
        {{ errorMsg }}
      </div>

      <textarea
        v-model="content"
        class="edit-textarea"
        :disabled="saving"
        spellcheck="false"
      />
      <div class="edit-actions">
        <button class="ws-btn ws-btn-primary" :disabled="saving" @click="handleSave">
          {{ saving ? t('common.saving') : t('common.save') }}
        </button>
        <button class="ws-btn" :disabled="saving" @click="emit('close')">
          {{ t('common.cancel') }}
        </button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.edit-block {
  margin-top: 16px;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  padding: 12px;
}
.edit-tip {
  font-size: 10.5px;
  color: var(--amber, oklch(0.62 0.14 70));
  background: var(--amber-bg, oklch(0.92 0.04 70));
  padding: 5px 10px;
  border-radius: var(--radius);
  margin-bottom: 8px;
}
.edit-loading {
  font-size: 11px;
  color: var(--muted-foreground);
  padding: 16px 0;
  text-align: center;
}
.edit-warn {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--destructive);
  background: color-mix(in oklch, var(--destructive) 8%, var(--card));
  padding: 6px 10px;
  border-radius: var(--radius);
  margin-bottom: 8px;
}
.edit-error {
  font-size: 11px;
  color: var(--destructive);
  margin-bottom: 8px;
}
.edit-textarea {
  width: 100%;
  min-height: 200px;
  padding: 8px 10px;
  font-size: 11.5px;
  font-family: var(--font-mono);
  line-height: 1.6;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--card);
  color: var(--foreground);
  resize: vertical;
}
.edit-textarea:focus {
  outline: 2px solid color-mix(in oklch, var(--primary) 40%, transparent);
  outline-offset: -1px;
}
.edit-actions {
  display: flex;
  gap: 6px;
  margin-top: 10px;
}
.ws-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  padding: 4px 12px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--card);
  cursor: pointer;
  color: var(--foreground);
}
.ws-btn:hover {
  background: var(--muted);
}
.ws-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.ws-btn-primary {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}
.ws-btn-primary:hover {
  opacity: 0.9;
}
</style>
