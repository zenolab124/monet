<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useChannels, type ChannelInfo, APPLE_FM_CHANNEL_ID } from '@/composables/useChannels'

const props = defineProps<{
  channel: ChannelInfo | null
}>()

const emit = defineEmits<{
  (e: 'saved'): void
  (e: 'cancel'): void
}>()

const { t } = useI18n()
const { saveChannel, revealToken, probeResults } = useChannels()

const isNew = computed(() => props.channel === null)

const id = ref(props.channel?.id ?? '')
const name = ref(props.channel?.name ?? '')
const baseUrl = ref(props.channel?.baseUrl ?? '')
const authToken = ref('')
const note = ref(props.channel?.note ?? '')
const protocol = ref(props.channel?.protocol ?? 'anthropic')
const scope = ref(props.channel?.scope ?? 'full')
const agentModel = ref(props.channel?.agentModel ?? '')
const tokenVisible = ref(false)

const isVirtual = computed(() => props.channel?.id === APPLE_FM_CHANNEL_ID)

const modelOptions = computed(() => {
  const channelId = props.channel?.id ?? id.value
  const probeModels = probeResults.value[channelId]?.models ?? []
  const savedModels = props.channel?.availableModels ?? []
  return [...new Set([...probeModels, ...savedModels])].sort()
})

const saving = ref(false)
const formError = ref<string | null>(null)

const ID_PATTERN = /^[a-zA-Z0-9_-]{1,64}$/

onMounted(async () => {
  if (!isNew.value && props.channel) {
    const token = await revealToken(props.channel.id)
    if (token) authToken.value = token
  }
})

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
  if (!isVirtual.value && !baseUrl.value.trim()) {
    formError.value = t('settings.channelForm.baseUrlError')
    return
  }
  if (!isVirtual.value && protocol.value !== 'openai' && !authToken.value.trim()) {
    formError.value = t('settings.channelForm.tokenError')
    return
  }
  saving.value = true
  try {
    await saveChannel({
      id: trimmedId,
      name: name.value.trim(),
      baseUrl: isVirtual.value ? '' : baseUrl.value.trim().replace(/\/+$/, ''),
      authToken: authToken.value.trim() || undefined,
      note: note.value.trim() || undefined,
      protocol: protocol.value,
      scope: scope.value,
      agentModel: agentModel.value.trim() || undefined,
      availableModels: modelOptions.value.length > 0 ? modelOptions.value : undefined,
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
      <span class="form-label">{{ $t('settings.channelForm.protocolLabel') }}</span>
      <select v-model="protocol" class="form-input">
        <option value="anthropic">Anthropic Messages API</option>
        <option value="openai">OpenAI Chat Completions</option>
      </select>
    </label>

    <label class="form-field">
      <span class="form-label">Base URL <span class="text-muted-foreground font-normal">{{ $t('settings.channelForm.baseUrlHint') }}</span></span>
      <input
        v-model="baseUrl"
        type="text"
        placeholder="https://api.example.com/anthropic"
        class="form-input font-mono"
        spellcheck="false"
      />
    </label>

    <div class="form-field">
      <span class="form-label">Auth Token <span v-if="protocol === 'openai'" class="text-muted-foreground/60 font-normal italic">{{ $t('settings.channelForm.tokenOptional') }}</span></span>
      <div class="relative">
        <input
          v-model="authToken"
          :type="tokenVisible ? 'text' : 'password'"
          placeholder="sk-…"
          class="form-input font-mono w-full pr-8"
          autocomplete="off"
        />
        <button
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
          @click="tokenVisible = !tokenVisible"
        >
          <span :class="tokenVisible ? 'i-carbon-view-off' : 'i-carbon-view'" class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <label class="form-field">
      <span class="form-label">{{ $t('settings.channelForm.scopeLabel') }}</span>
      <select v-model="scope" class="form-input">
        <option value="full">{{ $t('settings.channelForm.scopeFullHint') }}</option>
        <option value="agent-only">{{ $t('settings.channelForm.scopeAgentOnlyHint') }}</option>
      </select>
    </label>

    <div class="form-field">
      <span class="form-label">Agent Model <span class="text-muted-foreground/60 font-normal italic">{{ $t('common.optional') }}</span></span>
      <input
        v-model="agentModel"
        type="text"
        :list="`agent-model-opts-${id}`"
        placeholder="e.g. gpt-4o-mini"
        class="form-input font-mono"
        spellcheck="false"
      />
      <datalist :id="`agent-model-opts-${id}`">
        <option v-for="m in modelOptions" :key="m" :value="m" />
      </datalist>
      <span class="text-[10px] text-muted-foreground/70">{{ $t('settings.channelForm.agentModelHint') }}</span>
    </div>

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
</style>
