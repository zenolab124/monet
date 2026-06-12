<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import {
  MODELS,
  inferModel,
  DEFAULT_CONTEXT,
  type ModelInfo,
} from '@/utils/modelContext'
import { useCliDefaults, refreshCliDefaults } from '@/composables/useCliDefaults'

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

/** 当前模型解析结果(null = 清单外的自定义/未收录模型) */
const currentModel = computed(() => inferModel(props.current))

const { cliDefaults } = useCliDefaults()

/** CLI 默认模型展示标签(settings.json model 字段可为别名,经清单解析;未收录显原名) */
const cliDefaultModelLabel = computed(() => {
  const m = cliDefaults.value.model
  if (!m) return null
  return inferModel(m)?.label ?? m
})

/**
 * 按钮标签:清单命中显 label,清单外自定义显原始名;
 * 会话未选且无记录(新会话)时显「默认 · <CLI 真值>」,免去用户猜测默认是什么
 */
const currentLabel = computed(() => {
  if (currentModel.value) return currentModel.value.label
  if (props.current) return props.current
  return cliDefaultModelLabel.value ? `默认 · ${cliDefaultModelLabel.value}` : '默认'
})

/**
 * 下拉项 = 模型清单 + 当前自定义模型(若清单外):
 * 会话在用清单外模型(CLI 自定义设置/代理模型)时附加为可选项,原名展示。
 */
const items = computed<ModelInfo[]>(() => {
  if (props.current && !currentModel.value) {
    return [
      ...MODELS,
      { id: props.current, label: props.current, contextWindow: DEFAULT_CONTEXT },
    ]
  }
  return MODELS
})

/** 当前选中项在列表中的索引(用于打勾对比):优先原始字符串精确命中(自定义项) */
const currentIndex = computed(() => {
  const cur = props.current?.toLowerCase()
  if (!cur) return -1
  const exact = items.value.findIndex(m => m.id === cur)
  if (exact >= 0) return exact
  return currentModel.value
    ? items.value.findIndex(m => m.id === currentModel.value!.id)
    : -1
})

function toggle() {
  open.value = !open.value
  if (open.value) {
    // settings.json 是活文件:每次打开下拉重读,「默认」标签不显示过期值
    refreshCliDefaults()
    // 默认聚焦到当前项;若当前项无效(老会话),聚焦清单首项
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
  const m = items.value[index]
  if (!m) return
  emit('select', m.id)
  close()
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value + 1) % items.value.length
      focusListItem(focusedIndex.value)
      break
    case 'ArrowUp':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value - 1 + items.value.length) % items.value.length
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
        v-for="(m, i) in items"
        :key="m.id"
        data-item
        role="option"
        tabindex="-1"
        :aria-selected="i === currentIndex"
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground focus:outline-none"
        :class="{ 'mt-1 pt-1.5 border-t border-border': i > 0 && !!m.legacy !== !!items[i - 1].legacy }"
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
