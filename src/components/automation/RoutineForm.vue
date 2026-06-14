<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoutines, type RoutineDefinition } from '@/composables/useRoutines'

const props = defineProps<{
  routine: RoutineDefinition | null
}>()

const emit = defineEmits<{
  (e: 'saved'): void
  (e: 'cancel'): void
}>()

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
    formError.value = '名称不能为空'
    return
  }
  if (!cronExpression.value.trim()) {
    formError.value = 'Cron 表达式不能为空（请输入时间计划并点击 AI 识别）'
    return
  }
  if (!prompt.value.trim()) {
    formError.value = '指令不能为空'
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
    <div class="text-xs font-medium">{{ isNew ? '新建定时任务' : `编辑 · ${routine!.name}` }}</div>

    <label class="form-field">
      <span class="form-label">名称</span>
      <input v-model="name" type="text" placeholder="例如：每日 PR 检查" class="form-input" />
    </label>

    <div class="form-field">
      <span class="form-label">时间计划</span>
      <div class="flex gap-1.5">
        <input
          v-model="scheduleText"
          type="text"
          placeholder="自然语言，如「每天早上9点」「每周一下午3点」"
          class="form-input flex-1"
          @keydown.enter.prevent="onParseSchedule"
        />
        <button
          :disabled="parsing || !scheduleText.trim()"
          class="px-2 py-1 text-[11px] rounded-md border border-border bg-card text-foreground hover:shadow-paper transition-shadow disabled:opacity-50 whitespace-nowrap"
          @click="onParseSchedule"
        >
          <span v-if="parsing" class="i-carbon-renew w-3 h-3 inline-block animate-spin align-middle mr-0.5" />
          AI 识别
        </button>
      </div>
      <p v-if="parseError" class="text-[11px] text-destructive mt-0.5">{{ parseError }}</p>
    </div>

    <label class="form-field">
      <span class="form-label">Cron 表达式（分 时 日 月 周）</span>
      <input
        v-model="cronExpression"
        type="text"
        placeholder="例如：0 9 * * *"
        class="form-input font-mono"
        spellcheck="false"
      />
    </label>

    <label class="form-field">
      <span class="form-label">指令（发送给 Claude 的 prompt）</span>
      <textarea
        v-model="prompt"
        rows="3"
        placeholder="例如：检查我所有 GitHub 仓库的 open PR 状态，给出摘要"
        class="form-input resize-y"
      />
    </label>

    <label class="flex items-center gap-2 text-xs cursor-pointer">
      <input v-model="enabled" type="checkbox" class="accent-primary" />
      <span>创建后立即启用</span>
    </label>

    <p v-if="formError" class="text-xs text-destructive">{{ formError }}</p>

    <div class="flex items-center gap-2 justify-end">
      <button
        class="px-2.5 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        @click="emit('cancel')"
      >
        取消
      </button>
      <button
        :disabled="saving"
        class="px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow disabled:opacity-50"
        @click="onSave"
      >
        {{ saving ? '保存中…' : '保存' }}
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
