<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { MODELS, inferModel } from '@/utils/modelContext'

const props = defineProps<{
  /** 当前模型 ID(后端给的 model 字符串,可能是别名也可能是完整名) */
  current: string | null
}>()

const emit = defineEmits<{
  (e: 'select', modelId: string): void
}>()

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

/** 当前模型解析结果 */
const currentModel = computed(() => inferModel(props.current))
const currentLabel = computed(() => currentModel.value?.label ?? '未知')

/** 当前选中项在列表中的索引(用于打勾对比) */
const currentIndex = computed(() =>
  currentModel.value ? MODELS.findIndex(m => m.id === currentModel.value!.id) : -1,
)

function toggle() {
  open.value = !open.value
  if (open.value) {
    // 默认聚焦到当前项;若当前项无效(老会话),按 PRD 默认聚焦 sonnet(index 0)
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value : 0
    nextTick(() => focusListItem(focusedIndex.value))
  }
}

function close() {
  open.value = false
  // 关闭后焦点回到触发按钮
  buttonRef.value?.focus()
}

function selectAt(index: number) {
  const m = MODELS[index]
  if (!m) return
  emit('select', m.id)
  close()
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value + 1) % MODELS.length
      focusListItem(focusedIndex.value)
      break
    case 'ArrowUp':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value - 1 + MODELS.length) % MODELS.length
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

/** 点击外部关闭 */
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
      :title="`模型:${currentLabel}`"
      :aria-haspopup="'listbox'"
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="i-carbon-machine-learning-model w-3.5 h-3.5" />
      <span class="truncate max-w-32">{{ currentLabel }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground" />
    </button>

    <ul
      v-if="open"
      role="listbox"
      class="absolute top-full left-0 mt-1 z-50 min-w-36 py-1 rounded-md border border-border
             shadow-paper-lifted bg-popover"
    >
      <li
        v-for="(m, i) in MODELS"
        :key="m.id"
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
        <span class="flex-1">{{ m.label }}</span>
      </li>
    </ul>
  </div>
</template>
