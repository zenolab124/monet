<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import type { EffortLevel, EffortSetting } from '@/composables/useSessionSettings'

const props = defineProps<{
  /** null = 本会话未设置(按应用默认行为发送);'ultracode' = 超档(经 --settings 注入) */
  current: EffortSetting
}>()

const emit = defineEmits<{
  (e: 'select', effort: EffortSetting): void
}>()

interface EffortOption {
  value: NonNullable<EffortSetting>
  label: string
}

const EFFORT_LABELS: Record<EffortLevel, string> = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  xhigh: 'xHigh',
  max: 'Max',
}

/** 五档(按 PRD FR-006 L327) + Ultracode 超档。「跟随 CLI/默认值」归设置层,不在会话选项中 */
const OPTIONS: EffortOption[] = [
  ...(Object.entries(EFFORT_LABELS) as [EffortLevel, string][]).map(
    ([value, label]) => ({ value, label }),
  ),
  { value: 'ultracode' as const, label: 'Ultracode' },
]

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

const currentIndex = computed(() => OPTIONS.findIndex(o => o.value === props.current))
/** 未设置的会话显示「默认」:具体取值由应用默认决定,不在此处解释 */
const currentLabel = computed(() => {
  const o = OPTIONS.find(o => o.value === props.current)
  return o ? o.label : '默认'
})

function toggle() {
  open.value = !open.value
  if (open.value) {
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value : 0
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
