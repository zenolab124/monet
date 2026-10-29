<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { buildEngineChoices } from '@/engines/engineChoices'
import { useEngines } from '@/engines/useEngines'
import EngineChoicePanel from './EngineChoicePanel.vue'

const props = defineProps<{
  /** 当前赛道所属实例的 key */
  currentInstanceKey: string
  selectingEngineId: string | null
}>()

const emit = defineEmits<{
  (e: 'select', instanceKey: string): void
}>()

const { t } = useI18n()
const { engines, health, loading, errors, refreshEngines } = useEngines()

const choices = computed(() => buildEngineChoices({
  engines: engines.value,
  health: health.value,
  errors: errors.value,
  loading: loading.value,
}).map(choice => ({
  ...choice,
  description: t(`workbench.enginePicker.${choice.engineId === 'claude-code' ? 'claudeDescription' : 'codexDescription'}`),
  current: props.currentInstanceKey === choice.id,
})))

onMounted(() => {
  if (engines.value.length === 0) void refreshEngines()
})
</script>

<template>
  <EngineChoicePanel
    :title="t('workbench.race.chooseEngine')"
    :description="t('workbench.race.chooseEngineDescription')"
    :choices="choices"
    :selecting-engine="selectingEngineId"
    @select="emit('select', $event)"
  />
</template>
