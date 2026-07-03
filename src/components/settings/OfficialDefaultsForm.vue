<script setup lang="ts">
import { ref } from 'vue'
import { useChannels, type ChannelInfo } from '@/composables/useChannels'
import { MODELS, MODEL_ALIASES } from '@/utils/modelContext'

/**
 * official 渠道的轻量编辑:只有「默认模型 + 默认思考强度」两个预设字段
 * (无 Base URL/Token——OAuth 零注入),保存走 set_official_defaults 存渠道元数据。
 */
const props = defineProps<{
  channel: ChannelInfo
}>()

const emit = defineEmits<{
  (e: 'saved'): void
  (e: 'cancel'): void
}>()

const { setOfficialDefaults } = useChannels()

const defaultModel = ref(props.channel.defaultModel ?? '')
const defaultEffort = ref(props.channel.defaultEffort ?? '')
const saving = ref(false)
const formError = ref<string | null>(null)

/** 候选 = 官方角色 alias(始终最新) + 钉版本完整 ID */
const modelCandidates = [
  ...Object.keys(MODEL_ALIASES),
  'sonnet[1m]',
  ...MODELS.map(m => m.id),
]

async function onSave() {
  formError.value = null
  saving.value = true
  try {
    await setOfficialDefaults(defaultModel.value.trim() || null, defaultEffort.value || null)
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
    <div class="text-xs font-medium">{{ $t('settings.officialDefaults.title') }}</div>
    <p class="text-[10px] text-muted-foreground/70 leading-snug">{{ $t('settings.officialDefaults.intro') }}</p>

    <div class="grid grid-cols-2 gap-2">
      <div class="form-field">
        <span class="form-label">{{ $t('settings.channelForm.defaultModelLabel') }} <span class="text-muted-foreground/60 font-normal italic">{{ $t('common.optional') }}</span></span>
        <input
          v-model="defaultModel"
          type="text"
          list="official-default-model-opts"
          :placeholder="$t('settings.channelForm.defaultModelPlaceholder')"
          class="form-input font-mono"
          spellcheck="false"
        />
        <datalist id="official-default-model-opts">
          <option v-for="m in modelCandidates" :key="m" :value="m" />
        </datalist>
      </div>
      <div class="form-field">
        <span class="form-label">{{ $t('settings.channelForm.defaultEffortLabel') }} <span class="text-muted-foreground/60 font-normal italic">{{ $t('common.optional') }}</span></span>
        <select v-model="defaultEffort" class="form-input">
          <option value="">{{ $t('settings.channelForm.defaultEffortNone') }}</option>
          <option value="low">Low</option>
          <option value="medium">Medium</option>
          <option value="high">High</option>
          <option value="xhigh">xHigh</option>
          <option value="max">Max</option>
          <option value="ultracode">Ultracode</option>
        </select>
      </div>
    </div>
    <p class="text-[10px] text-muted-foreground/60 leading-snug">{{ $t('settings.channelForm.defaultsHint') }}</p>

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
