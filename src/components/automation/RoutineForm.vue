<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoutines, type RoutineDefinition } from '@/composables/useRoutines'

const props = defineProps<{
  routine: RoutineDefinition | null
}>()

const emit = defineEmits<{
  (e: 'saved'): void
  (e: 'cancel'): void
}>()

const { t } = useI18n()
const { createRoutine, updateRoutine, parseNaturalSchedule } = useRoutines()

const isNew = computed(() => props.routine === null)

const name = ref(props.routine?.name ?? '')
const scheduleText = ref(props.routine?.originalText ?? '')
const cronExpression = ref(props.routine?.cronExpression ?? '')
const prompt = ref(props.routine?.prompt ?? '')
const enabled = ref(props.routine?.enabled ?? true)

const saving = ref(false)
const parsing = ref(false)
const formError = ref<string | null>(null)
const parseError = ref<string | null>(null)

async function onParseSchedule() {
  const text = scheduleText.value.trim()
  if (!text) return
  parsing.value = true
  parseError.value = null
  try {
    cronExpression.value = await parseNaturalSchedule(text)
  } catch (e) {
    parseError.value = String(e)
  } finally {
    parsing.value = false
  }
}

async function onSave() {
  formError.value = null
  if (!name.value.trim()) {
    formError.value = t('automation.routineForm.nameRequired')
    return
  }
  if (!cronExpression.value.trim()) {
    formError.value = t('automation.routineForm.cronRequired')
    return
  }
  if (!prompt.value.trim()) {
    formError.value = t('automation.routineForm.promptRequired')
    return
  }

  saving.value = true
  try {
    if (isNew.value) {
      await createRoutine({
        name: name.value.trim(),
        cronExpression: cronExpression.value.trim(),
        originalText: scheduleText.value.trim(),
        prompt: prompt.value.trim(),
        enabled: enabled.value,
      })
    } else {
      await updateRoutine(props.routine!.id, {
        name: name.value.trim(),
        cronExpression: cronExpression.value.trim(),
        originalText: scheduleText.value.trim(),
        prompt: prompt.value.trim(),
        enabled: enabled.value,
      })
    }
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
    <div class="text-xs font-medium">{{ isNew ? $t('automation.routineForm.newTitle') : $t('automation.routineForm.editTitle', { name: routine!.name }) }}</div>

    <label class="form-field">
      <span class="form-label">{{ $t('automation.routineForm.nameLabel') }}</span>
      <input v-model="name" type="text" :placeholder="$t('automation.routineForm.namePlaceholder')" class="form-input" />
    </label>

    <div class="form-field">
      <span class="form-label">{{ $t('automation.routineForm.scheduleLabel') }}</span>
      <div class="flex gap-1.5">
        <input
          v-model="scheduleText"
          type="text"
          :placeholder="$t('automation.routineForm.schedulePlaceholder')"
          class="form-input flex-1"
          @keydown.enter.prevent="onParseSchedule"
        />
        <button
          :disabled="parsing || !scheduleText.trim()"
          class="px-2 py-1 text-[11px] rounded-md border border-border bg-card text-foreground hover:shadow-paper transition-shadow disabled:opacity-50 whitespace-nowrap"
          @click="onParseSchedule"
        >
          <span v-if="parsing" class="i-carbon-renew w-3 h-3 inline-block animate-spin align-middle mr-0.5" />
          {{ $t('automation.routineForm.aiParse') }}
        </button>
      </div>
      <p v-if="parseError" class="text-[11px] text-destructive mt-0.5">{{ parseError }}</p>
    </div>

    <label class="form-field">
      <span class="form-label">{{ $t('automation.routineForm.cronLabel') }}</span>
      <input
        v-model="cronExpression"
        type="text"
        :placeholder="$t('automation.routineForm.cronPlaceholder')"
        class="form-input font-mono"
        spellcheck="false"
      />
    </label>

    <label class="form-field">
      <span class="form-label">{{ $t('automation.routineForm.promptLabel') }}</span>
      <textarea
        v-model="prompt"
        rows="3"
        :placeholder="$t('automation.routineForm.promptPlaceholder')"
        class="form-textarea"
      />
    </label>

    <label class="flex items-center gap-2 text-xs cursor-pointer">
      <input v-model="enabled" type="checkbox" class="accent-primary" />
      <span>{{ $t('automation.routineForm.enableOnCreate') }}</span>
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
