<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'

export interface SelectOption {
  value: string
  label: string
  sub?: string
}

const props = withDefaults(defineProps<{
  options: SelectOption[]
  modelValue: string
  placeholder?: string
  mono?: boolean
  editable?: boolean
}>(), { placeholder: '', mono: false, editable: false })

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
}>()

const open = ref(false)
const containerRef = ref<HTMLElement>()
const inputRef = ref<HTMLInputElement>()
const focusedIndex = ref(0)
const inputText = ref(props.modelValue)

watch(() => props.modelValue, (v) => { inputText.value = v })

const filteredOptions = computed(() => {
  if (!props.editable || !inputText.value) return props.options
  const q = inputText.value.toLowerCase()
  return props.options.filter(o => o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q))
})

const currentOption = computed(() => props.options.find(o => o.value === props.modelValue))
const currentIndex = computed(() => filteredOptions.value.findIndex(o => o.value === props.modelValue))

function toggle() {
  open.value = !open.value
  if (open.value) {
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value : 0
    if (props.editable) {
      nextTick(() => inputRef.value?.focus())
    } else {
      nextTick(() => focusItem(focusedIndex.value))
    }
  }
}

function openMenu() {
  if (!open.value) {
    open.value = true
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value : 0
  }
}

function close() { open.value = false }

function select(index: number) {
  const o = filteredOptions.value[index]
  if (!o) return
  inputText.value = o.value
  emit('update:modelValue', o.value)
  close()
}

function onInputChange() {
  openMenu()
}

function onInputBlur() {
  if (inputText.value !== props.modelValue) {
    emit('update:modelValue', inputText.value)
  }
}

function onKeydown(e: KeyboardEvent) {
  const opts = filteredOptions.value
  if (!open.value) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { openMenu(); e.preventDefault() }
    return
  }
  switch (e.key) {
    case 'ArrowDown': e.preventDefault(); focusedIndex.value = (focusedIndex.value + 1) % opts.length; if (!props.editable) focusItem(focusedIndex.value); break
    case 'ArrowUp': e.preventDefault(); focusedIndex.value = (focusedIndex.value - 1 + opts.length) % opts.length; if (!props.editable) focusItem(focusedIndex.value); break
    case 'Enter': e.preventDefault(); if (opts.length) select(focusedIndex.value); else { emit('update:modelValue', inputText.value); close() }; break
    case 'Escape': e.preventDefault(); close(); break
  }
}

function focusItem(index: number) {
  nextTick(() => {
    containerRef.value?.querySelectorAll<HTMLElement>('[data-item]')[index]?.focus()
  })
}

function onOutsideClick(e: MouseEvent) {
  if (open.value && containerRef.value && !containerRef.value.contains(e.target as Node)) {
    if (props.editable && inputText.value !== props.modelValue) {
      emit('update:modelValue', inputText.value)
    }
    open.value = false
  }
}

onMounted(() => document.addEventListener('mousedown', onOutsideClick))
onUnmounted(() => document.removeEventListener('mousedown', onOutsideClick))
</script>

<template>
  <div ref="containerRef" class="paper-select" @keydown="onKeydown">
    <!-- editable: input trigger -->
    <div v-if="editable" class="paper-select-trigger" @click="toggle">
      <input
        ref="inputRef"
        v-model="inputText"
        :placeholder="placeholder"
        :class="['paper-select-input', { 'font-mono': mono }]"
        spellcheck="false"
        @input="onInputChange"
        @blur="onInputBlur"
        @focus="openMenu"
      />
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground shrink-0 transition-transform" :class="{ 'rotate-180': open }" />
    </div>
    <!-- readonly: button trigger -->
    <button v-else type="button" class="paper-select-trigger" @click="toggle">
      <span :class="['truncate flex-1 text-left', { 'font-mono': mono }]">{{ currentOption?.label || placeholder }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground shrink-0 transition-transform" :class="{ 'rotate-180': open }" />
    </button>

    <ul v-if="open && filteredOptions.length" class="paper-select-menu">
      <li
        v-for="(o, i) in filteredOptions"
        :key="o.value"
        data-item
        tabindex="-1"
        class="paper-select-item"
        :class="{ 'paper-select-item-active': o.value === modelValue, 'paper-select-item-focused': i === focusedIndex && editable }"
        @click="select(i)"
        @mouseenter="focusedIndex = i"
      >
        <span class="w-3 h-3 shrink-0" :class="o.value === modelValue ? 'i-carbon-checkmark text-primary' : ''" />
        <span class="flex-1 min-w-0">
          <span :class="['truncate block', { 'font-mono': mono }]">{{ o.label }}</span>
          <span v-if="o.sub" class="text-[9px] text-muted-foreground/60 truncate block">{{ o.sub }}</span>
        </span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.paper-select {
  position: relative;
  display: inline-flex;
  width: 100%;
}
.paper-select-trigger {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 4px 8px;
  font-size: 11px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--background);
  color: var(--foreground);
  cursor: pointer;
  transition: all 0.15s;
}
.paper-select-trigger:hover {
  border-color: var(--primary);
  box-shadow: var(--shadow-paper);
}
.paper-select-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 11px;
  color: var(--foreground);
}
.paper-select-input::placeholder {
  color: var(--muted-foreground);
}
.paper-select-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 50;
  max-height: 200px;
  overflow-y: auto;
  padding: 3px;
  margin: 0;
  list-style: none;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--popover);
  box-shadow: var(--shadow-paper-lifted);
}
.paper-select-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
  font-size: 11px;
  color: var(--muted-foreground);
  border-radius: calc(var(--radius) - 2px);
  cursor: pointer;
  outline: none;
}
.paper-select-item:hover,
.paper-select-item:focus,
.paper-select-item-focused {
  background: var(--muted);
  color: var(--foreground);
}
.paper-select-item-active {
  color: var(--foreground);
}
</style>
