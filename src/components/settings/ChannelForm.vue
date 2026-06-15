<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useChannels, type ChannelInfo } from '@/composables/useChannels'

/**
 * 渠道新建/编辑表单。
 * 安全口径:authToken 是 write-only——编辑时不回显原值,留空 = 保持不变;
 * 输入框用 password 型避免肩窥,保存后即出渲染层。
 */
const props = defineProps<{
  /** null = 新建 */
  channel: ChannelInfo | null
}>()

const emit = defineEmits<{
  (e: 'saved'): void
  (e: 'cancel'): void
}>()

const { t } = useI18n()
const { saveChannel } = useChannels()

const isNew = computed(() => props.channel === null)

const id = ref(props.channel?.id ?? '')
const name = ref(props.channel?.name ?? '')
const baseUrl = ref(props.channel?.baseUrl ?? '')
const authToken = ref('')
const note = ref(props.channel?.note ?? '')

const saving = ref(false)
const formError = ref<string | null>(null)

const ID_PATTERN = /^[a-zA-Z0-9_-]{1,64}$/

async function onSave() {
  formError.value = null
  const trimmedId = id.value.trim()
  if (!ID_PATTERN.test(trimmedId) || trimmedId === 'official') {
    formError.value = t('settings.channelForm.idError')
    return
  }
  if (!name.value.trim()) {
    formError.value = t('settings.channelForm.nameError')
    return
  }
  if (!baseUrl.value.trim()) {
    formError.value = t('settings.channelForm.baseUrlError')
    return
  }
  if (isNew.value && !authToken.value.trim()) {
    formError.value = t('settings.channelForm.tokenError')
    return
  }
  saving.value = true
  try {
    await saveChannel({
      id: trimmedId,
      name: name.value.trim(),
      baseUrl: baseUrl.value.trim(),
      authToken: authToken.value.trim() || undefined,
      note: note.value.trim() || undefined,
    })
    emit('saved')
  } catch (e) {
    formError.value = String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="rounded-md border border-border bg-popover p-3 flex flex-col gap-2.5">
    <div class="text-xs font-medium">{{ isNew ? $t('settings.channelForm.newTitle') : $t('settings.channelForm.editTitle', { id: channel!.id }) }}</div>

    <label class="form-field">
      <span class="form-label">{{ $t('settings.channelForm.idLabel') }}</span>
      <input
        v-model="id"
        :disabled="!isNew"
        type="text"
        :placeholder="$t('settings.channelForm.idPlaceholder')"
        class="form-input disabled:opacity-50"
        spellcheck="false"
      />
    </label>

    <label class="form-field">
      <span class="form-label">{{ $t('settings.channelForm.nameLabel') }}</span>
      <input v-model="name" type="text" :placeholder="$t('settings.channelForm.namePlaceholder')" class="form-input" />
    </label>

    <label class="form-field">
      <span class="form-label">Base URL</span>
      <input
        v-model="baseUrl"
        type="text"
        placeholder="https://api.example.com/anthropic"
        class="form-input font-mono"
        spellcheck="false"
      />
    </label>

    <label class="form-field">
      <span class="form-label">Auth Token</span>
      <input
        v-model="authToken"
        type="password"
        :placeholder="isNew ? 'sk-…(写入 ANTHROPIC_AUTH_TOKEN)' : $t('settings.channelForm.tokenKeepPlaceholder')"
        class="form-input font-mono"
        autocomplete="off"
      />
    </label>

    <label class="form-field">
      <span class="form-label">{{ $t('settings.channelForm.noteLabel') }}</span>
      <input v-model="note" type="text" :placeholder="$t('common.optional')" class="form-input" />
    </label>

    <p v-if="formError" class="text-xs text-destructive">{{ formError }}</p>

    <div class="flex items-center gap-2 justify-end">
      <button
        class="px-2.5 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        @click="emit('cancel')"
      >
        {{ $t('common.cancel') }}
      </button>
      <button
        :disabled="saving"
        class="px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow disabled:opacity-50"
        @click="onSave"
      >
        {{ saving ? $t('common.saving') : $t('common.save') }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.form-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.form-label {
  font-size: 11px;
  color: var(--muted-foreground);
}
.form-input {
  padding: 6px 8px;
  font-size: 12px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--background);
  color: var(--foreground);
  transition: border-color 0.15s;
}
.form-input:focus {
  outline: none;
  border-color: var(--ring);
}
.form-input::placeholder {
  color: var(--muted-foreground);
  opacity: 0.6;
}
</style>
