<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'

defineProps<{ disabled?: boolean }>()
const emit = defineEmits<{ changed: []; busy: [value: boolean] }>()
const { t } = useI18n()

interface BinaryInfo {
  manualPath: string | null
  resolvedPath: string | null
  manualValid: boolean
}

const info = ref<BinaryInfo | null>(null)
const input = ref('')
const busy = ref(false)
const error = ref('')
const saved = ref(false)

function apply(next: BinaryInfo) {
  info.value = next
  input.value = next.manualPath ?? ''
}

async function perform(command: string, args?: Record<string, unknown>) {
  if (busy.value) return
  busy.value = true
  emit('busy', true)
  error.value = ''
  saved.value = false
  try {
    apply(await invoke<BinaryInfo>(command, args))
    if (command !== 'get_codex_binary_info') {
      saved.value = command === 'set_codex_binary_path'
      emit('changed')
    }
  } catch (cause) {
    error.value = String(cause)
  } finally {
    busy.value = false
    emit('busy', false)
  }
}

async function browse(directory: boolean) {
  busy.value = true
  emit('busy', true)
  error.value = ''
  saved.value = false
  try {
    const selected = await open({ directory, multiple: false })
    if (typeof selected === 'string') input.value = selected
  } catch (cause) {
    error.value = String(cause)
  } finally {
    busy.value = false
    emit('busy', false)
  }
}

onMounted(() => perform('get_codex_binary_info'))
</script>

<template>
  <form class="mt-2 border-t border-border pt-2" @submit.prevent="perform('set_codex_binary_path', { path: input.trim() || null })">
    <div class="flex items-center justify-between gap-2">
      <label for="codex-binary-path" class="text-[11px] font-medium">{{ t('settings.codexBin.title') }}</label>
      <button type="button" class="path-btn" :disabled="busy || disabled" @click="perform('redetect_codex_binary')">
        {{ t('settings.codexBin.redetect') }}
      </button>
    </div>
    <p id="codex-binary-hint" class="mt-1 text-[10px] text-muted-foreground">{{ t('settings.codexBin.hint') }}</p>
    <input
      id="codex-binary-path" v-model="input" type="text"
      class="form-input mt-1 w-full min-w-0 text-[11px]"
      :placeholder="t('settings.codexBin.placeholder')" :disabled="busy || disabled"
      aria-describedby="codex-binary-hint" :aria-invalid="!!error || (!!info?.manualPath && !info.manualValid)"
      @input="saved = false; error = ''"
    >
    <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
      <button type="button" class="path-btn" :disabled="busy || disabled" @click="browse(false)">{{ t('settings.codexBin.chooseFile') }}</button>
      <button type="button" class="path-btn" :disabled="busy || disabled" @click="browse(true)">{{ t('settings.codexBin.chooseDirectory') }}</button>
      <span class="flex-1" />
      <button type="button" class="path-btn" :disabled="busy || disabled || !info?.manualPath" @click="perform('set_codex_binary_path', { path: null })">{{ t('settings.codexBin.automatic') }}</button>
      <button type="submit" class="path-btn" :disabled="busy || disabled">{{ t('common.save') }}</button>
    </div>
    <p v-if="info?.resolvedPath" class="mt-1 break-all text-[10px] text-muted-foreground">{{ t('settings.codexBin.resolved', { path: info.resolvedPath }) }}</p>
    <p v-if="info?.manualPath && !info.manualValid" role="status" class="mt-1 text-[10px] text-destructive">{{ t('settings.codexBin.invalid') }}</p>
    <p v-if="error" role="alert" class="mt-1 break-words text-[10px] text-destructive">{{ error }}</p>
    <p v-else-if="saved" role="status" class="mt-1 text-[10px] text-primary">{{ t('settings.codexBin.saved') }}</p>
  </form>
</template>

<style scoped>
.path-btn {
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 8px;
  font-size: 11px;
  background: var(--background);
  color: var(--foreground);
  cursor: pointer;
}
.path-btn:hover:not(:disabled) { border-color: var(--foreground); }
.path-btn:disabled { opacity: 0.5; cursor: default; }
</style>
