<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import type { EffortLevel } from '@/composables/useSessionSettings'

const props = defineProps<{
  current: EffortLevel
}>()

const emit = defineEmits<{
  (e: 'select', effort: EffortLevel): void
}>()

interface EffortOption {
  value: EffortLevel
  label: string
}

/** 五档(UI 标签 → CLI 值),按 PRD FR-006 L327 */
const OPTIONS: EffortOption[] = [
  { value: 'low', label: '轻量' },
  { value: 'medium', label: '标准' },
  { value: 'high', label: '加强' },
  { value: 'xhigh', label: '高强度' },
  { value: 'max', label: '最高' },
]

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

const currentIndex = computed(() => OPTIONS.findIndex(o => o.value === props.current))
const currentLabel = computed(() => {
  const o = OPTIONS.find(o => o.value === props.current)
  return o ? o.label : '标准'
})

function toggle() {
  open.value = !open.value
  if (open.value) {
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value : 1 // 默认聚焦"标准"
    nextTick(() => focusListItem(focusedIndex.value))
  }
}

function close() {
  open.value = false
  buttonRef.value?.focus()
}

function selectAt(index: number) {
  const o = OPTIONS[index]
  if (!o) return
  emit('select', o.value)
  close()
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value + 1) % OPTIONS.length
      focusListItem(focusedIndex.value)
      break
    case 'ArrowUp':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value - 1 + OPTIONS.length) % OPTIONS.length
      focusListItem(focusedIndex.value)
      break
    case 'Enter':
      e.preventDefault()
      selectAt(focusedIndex.value)
      break
    case 'Escape':
      e.preventDefault()
      close()
      break
  }
}

function focusListItem(index: number) {
  nextTick(() => {
    const el = containerRef.value?.querySelectorAll<HTMLElement>('[data-item]')[index]
    el?.focus()
  })
}

function onDocumentClick(e: MouseEvent) {
  if (!open.value) return
  const target = e.target as Node
  if (containerRef.value && !containerRef.value.contains(target)) {
    open.value = false
  }
}

onMounted(() => {
  document.addEventListener('mousedown', onDocumentClick)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onDocumentClick)
})
</script>

<template>
  <div ref="containerRef" class="relative inline-flex" @keydown="onKeydown">
    <button
      ref="buttonRef"
      type="button"
      class="px-2 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted
             transition-colors flex items-center gap-1 border border-border"
      :title="`努力等级:${currentLabel}`"
      :aria-haspopup="'listbox'"
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="i-carbon-meter w-3.5 h-3.5" />
      <span class="truncate">{{ currentLabel }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground" />
    </button>

    <ul
      v-if="open"
      role="listbox"
      class="absolute top-full left-0 mt-1 z-50 min-w-28 py-1 rounded-md border border-border
             shadow-paper-lifted bg-popover"
    >
      <li
        v-for="(o, i) in OPTIONS"
        :key="o.value"
        data-item
        role="option"
        tabindex="-1"
        :aria-selected="i === currentIndex"
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground focus:outline-none"
        @click="selectAt(i)"
        @mouseenter="focusedIndex = i"
      >
        <span
          class="w-3 h-3 shrink-0"
          :class="i === currentIndex ? 'i-carbon-checkmark text-primary' : ''"
        />
        <span class="flex-1">{{ o.label }}</span>
      </li>
    </ul>
  </div>
</template>
