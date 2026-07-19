<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useMemory } from '@/composables/useMemory'

/**
 * 记忆删除确认对话框（v2.9.0 FR-009）：
 * 文案明确说明「移入 Monet 回收站（~/.monet/trash），不会永久删除」。
 */

const { t } = useI18n()
const { selectedEntry, deleteMemory } = useMemory()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const deleting = ref(false)
const errorMsg = ref<string | null>(null)

async function handleDelete() {
  deleting.value = true
  errorMsg.value = null
  const result = await deleteMemory()
  deleting.value = false
  if (result.ok) {
    emit('close')
  } else {
    errorMsg.value = result.error ?? t('common.loadFailed')
  }
}
</script>

<template>
  <div class="dialog-block">
    <h3>{{ t('memory.deleteTitle') }}</h3>
    <p>{{ t('memory.deleteConfirm', { name: selectedEntry?.name ?? '' }) }}</p>
    <p class="dialog-note">{{ t('memory.deleteNote') }}</p>

    <div v-if="errorMsg" class="dialog-error">{{ errorMsg }}</div>

    <div class="dialog-actions">
      <button class="ws-btn ws-btn-danger" :disabled="deleting" @click="handleDelete">
        {{ deleting ? t('common.loading') : t('memory.confirmDelete') }}
      </button>
      <button class="ws-btn" :disabled="deleting" @click="emit('close')">
        {{ t('common.cancel') }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.dialog-block {
  background: var(--popover, var(--card));
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-lifted, var(--shadow-paper));
  padding: 16px 20px;
  margin-top: 16px;
  max-width: 400px;
}
.dialog-block h3 {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 6px;
}
.dialog-block p {
  font-size: 11.5px;
  line-height: 1.6;
  margin-bottom: 6px;
}
.dialog-note {
  font-size: 11px;
  color: var(--muted-foreground);
  margin-bottom: 10px;
}
.dialog-error {
  font-size: 11px;
  color: var(--destructive);
  margin-bottom: 8px;
}
.dialog-actions {
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
.ws-btn-danger {
  color: var(--destructive);
  border-color: color-mix(in oklch, var(--destructive) 40%, var(--border));
}
.ws-btn-danger:hover {
  background: color-mix(in oklch, var(--destructive) 8%, var(--card));
}
</style>
