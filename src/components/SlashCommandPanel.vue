<script setup lang="ts">
/**
 * 斜杠命令补全面板（FR-004）
 *
 * 由父组件控制 visible / query / position：
 *  - visible=true 时挂载并监听全局键盘（↑↓ Enter Esc）
 *  - 选中后通过 emit('select') 把命令交还父组件插入输入框
 *  - 用户按 Esc / 失焦时 emit('close')
 *
 * 注意：此组件 visible=true 时会拦截 ↑↓ Enter Esc，
 *      避免父组件的 textarea Enter 误发消息。
 */
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import {
  filterCommands,
  type SlashCommand,
} from '@/composables/useSlashCommands'

const props = defineProps<{
  /** 是否显示 */
  visible: boolean
  /** 当前输入（用于过滤），如 "/" 或 "/he" */
  query: string
  /** 可选的绝对定位坐标（由父组件计算输入框位置后传入） */
  position?: { top: number; left: number }
}>()

const emit = defineEmits<{
  (e: 'select', command: SlashCommand): void
  (e: 'close'): void
}>()

/** 当前过滤后的命令清单 */
const filtered = computed<SlashCommand[]>(() => filterCommands(props.query))

/** 当前高亮索引 */
const activeIndex = ref(0)

/** query 变化时重置选中位置 */
watch(
  () => [props.query, props.visible] as const,
  () => {
    activeIndex.value = 0
  },
)

/** 选项总数变化时夹逼索引 */
watch(filtered, (list) => {
  if (activeIndex.value >= list.length) {
    activeIndex.value = Math.max(0, list.length - 1)
  }
})

/** 选中某条命令 */
function selectAt(index: number) {
  const list = filtered.value
  if (index < 0 || index >= list.length) return
  emit('select', list[index])
}

/** 全局 keydown 拦截 */
function onKeydown(e: KeyboardEvent) {
  if (!props.visible) return
  const key = e.key
  // 拦截 4 个按键，无论是否有匹配项
  if (key === 'ArrowDown' || key === 'ArrowUp' || key === 'Enter' || key === 'Escape') {
    if (key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      emit('close')
      return
    }

    // 无匹配时：只处理 Esc，其它键交回父组件（让 Enter 发原文）
    if (filtered.value.length === 0) {
      // ArrowUp/Down 在空状态下无意义，吞掉避免光标乱跳
      if (key === 'ArrowUp' || key === 'ArrowDown') {
        e.preventDefault()
        e.stopPropagation()
      }
      return
    }

    e.preventDefault()
    e.stopPropagation()

    if (key === 'ArrowDown') {
      const len = filtered.value.length
      activeIndex.value = (activeIndex.value + 1) % len
    } else if (key === 'ArrowUp') {
      const len = filtered.value.length
      activeIndex.value = (activeIndex.value - 1 + len) % len
    } else if (key === 'Enter') {
      selectAt(activeIndex.value)
    }
  }
}

/** 仅在 visible=true 时挂载全局监听，避免不必要的开销 */
watch(
  () => props.visible,
  (v) => {
    if (v) {
      // 用 capture 确保比 textarea 的 keydown 先到，方便 stopPropagation
      window.addEventListener('keydown', onKeydown, { capture: true })
    } else {
      window.removeEventListener('keydown', onKeydown, { capture: true } as any)
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown, { capture: true } as any)
})

/** 绝对定位 style */
const positionStyle = computed(() => {
  if (!props.position) return undefined
  return {
    top: `${props.position.top}px`,
    left: `${props.position.left}px`,
  }
})
</script>

<template>
  <div
    v-if="visible"
    class="slash-panel rounded-md border border-divider bg-input shadow-lg overflow-hidden"
    :class="position ? 'fixed z-50' : 'absolute z-50'"
    :style="positionStyle"
    role="listbox"
    aria-label="斜杠命令补全"
  >
    <!-- 空状态 -->
    <div
      v-if="filtered.length === 0"
      class="px-3 py-2 text-xs text-default4"
    >
      无匹配，Enter 发送原文
    </div>

    <!-- 命令列表 -->
    <ul v-else class="py-1 max-h-72 overflow-y-auto">
      <li
        v-for="(cmd, i) in filtered"
        :key="cmd.name"
        role="option"
        :aria-selected="i === activeIndex"
        class="px-3 py-1.5 cursor-pointer flex items-baseline gap-2 transition-colors"
        :class="i === activeIndex ? 'bg-primary/10' : 'hover:bg-hover'"
        @mouseenter="activeIndex = i"
        @click="selectAt(i)"
      >
        <span class="text-sm font-mono text-primary shrink-0">/{{ cmd.name }}</span>
        <span
          v-if="cmd.hasArg && cmd.argHint"
          class="text-xs text-default4 font-mono shrink-0"
        >
          {{ cmd.argHint }}
        </span>
        <span class="text-xs text-default3 truncate">{{ cmd.hint }}</span>
      </li>
    </ul>

    <!-- 底部提示 -->
    <div
      v-if="filtered.length > 0"
      class="px-3 py-1 border-t border-divider text-2xs text-default4 flex items-center gap-3"
    >
      <span><kbd class="kbd">↑↓</kbd> 移动</span>
      <span><kbd class="kbd">Enter</kbd> 选择</span>
      <span><kbd class="kbd">Esc</kbd> 关闭</span>
    </div>
  </div>
</template>

<style scoped>
.slash-panel {
  min-width: 280px;
  max-width: 420px;
}

.text-2xs {
  font-size: 10px;
  line-height: 1.3;
}

.kbd {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 10px;
  padding: 0 4px;
  border: 1px solid var(--c-default4, rgba(127, 127, 127, 0.3));
  border-radius: 3px;
  background: transparent;
  color: var(--c-default3, inherit);
}
</style>
