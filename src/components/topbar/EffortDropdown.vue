<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { EffortLevel, EffortSetting } from '@/composables/useSessionSettings'
import { useCliDefaults, refreshCliDefaults } from '@/composables/useCliDefaults'
import type { ResolvedRunConfig } from '@/composables/useRunConfig'

/**
 * 「显示与发送同源」:按钮展示 runConfig 解析出的下次发送档位(会话覆盖 >
 * 渠道默认 > CLI 默认),菜单首项「默认」= 清除会话覆盖回落继承链。
 */
const props = defineProps<{
  /** 会话覆盖的档位(null = 未覆盖,跟随渠道/CLI 默认) */
  current: EffortSetting
  /** 运行配置解析结果(按钮显示 + 默认项预览) */
  runConfig: ResolvedRunConfig
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

/** 五档 + Ultracode 超档(具体覆盖项;「默认」继承项在菜单首位单列) */
const OPTIONS: EffortOption[] = [
  ...(Object.entries(EFFORT_LABELS) as [EffortLevel, string][]).map(
    ([value, label]) => ({ value, label }),
  ),
  { value: 'ultracode' as const, label: 'Ultracode' },
]

function labelOf(value: string | null | undefined): string | null {
  if (!value) return null
  return OPTIONS.find(o => o.value === value)?.label ?? null
}

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

/** 会话覆盖项在具体清单中的索引(打勾判定):无覆盖时勾落在「默认」项 */
const currentIndex = computed(() => OPTIONS.findIndex(o => o.value === props.current))

const { cliDefaults } = useCliDefaults()

/** CLI 层读数:ultracode 独立开关生效时覆盖 effortLevel(不判它会显示 Max 实跑 Ultracode) */
const cliLabel = computed(() => {
  if (cliDefaults.value.ultracode) return 'Ultracode'
  const lv = cliDefaults.value.effort_level
  return lv && lv in EFFORT_LABELS ? EFFORT_LABELS[lv as EffortLevel] : null
})

/** 菜单「默认」项的继承值预览:渠道默认 > CLI 默认 */
const inheritLabel = computed(
  () => labelOf(props.runConfig.channelDefaultEffort) ?? cliLabel.value,
)

/** 按钮标签 = 解析后的下次发送档位;CLI 层无读数时兜底 High(CLI 出厂默认) */
const currentLabel = computed(
  () => labelOf(props.runConfig.effort) ?? cliLabel.value ?? 'High',
)

/** 继承态(非会话覆盖):按钮弱化显示 + title 说明来源 */
const inherited = computed(() => props.runConfig.effortSource !== 'session')

const { t } = useI18n()

const sourceTitle = computed(() => {
  const name = currentLabel.value
  switch (props.runConfig.effortSource) {
    case 'channel': return t('topbar.sourceChannel', { name })
    case 'cli': return t('topbar.sourceCli', { name })
    default: return t('topbar.effortTitle', { name })
  }
})

/** 键盘导航总项数 = 「默认」项 + 具体项 */
const totalCount = computed(() => OPTIONS.length + 1)

function toggle() {
  open.value = !open.value
  if (open.value) {
    // settings.json 是活文件:每次打开下拉重读,「默认」标签不显示过期值
    refreshCliDefaults()
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value + 1 : 0
    nextTick(() => focusListItem(focusedIndex.value))
  }
}

function close() {
  open.value = false
  buttonRef.value?.focus()
}

/** index 0 = 「默认」项(清除覆盖);1.. = 具体档位项 */
function selectAt(index: number) {
  if (index === 0) {
    emit('select', null)
    close()
    return
  }
  const o = OPTIONS[index - 1]
  if (!o) return
  emit('select', o.value)
  close()
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value + 1) % totalCount.value
      focusListItem(focusedIndex.value)
      break
    case 'ArrowUp':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value - 1 + totalCount.value) % totalCount.value
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
      class="px-1.5 py-0.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted
             transition-colors flex items-center gap-1 border border-border"
      :title="sourceTitle"
      :aria-haspopup="'listbox'"
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="truncate max-w-20" :class="{ 'opacity-70': inherited }">{{ currentLabel }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground" />
    </button>

    <ul
      v-if="open"
      role="listbox"
      class="absolute top-full left-0 mt-1 z-50 min-w-28 py-1 rounded-md border border-border
             shadow-paper-lifted bg-popover"
    >
      <!-- 「默认」项:清除会话覆盖,跟随渠道/CLI 继承链 -->
      <li
        data-item
        role="option"
        tabindex="-1"
        :aria-selected="currentIndex < 0"
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground focus:outline-none"
        :title="runConfig.channelDefaultEffort ? $t('topbar.inheritFromChannel') : $t('topbar.inheritFromCli')"
        @click="selectAt(0)"
        @mouseenter="focusedIndex = 0"
      >
        <span
          class="w-3 h-3 shrink-0"
          :class="currentIndex < 0 ? 'i-carbon-checkmark text-primary' : ''"
        />
        <span class="flex-1">{{ $t('topbar.effortDefault') }}</span>
        <span v-if="inheritLabel" class="text-[10px] opacity-60">{{ inheritLabel }}</span>
      </li>

      <li
        v-for="(o, i) in OPTIONS"
        :key="o.value"
        data-item
        role="option"
        tabindex="-1"
        :aria-selected="i === currentIndex"
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground focus:outline-none"
        :class="{ 'mt-1 pt-1.5 border-t border-border': i === 0 }"
        @click="selectAt(i + 1)"
        @mouseenter="focusedIndex = i + 1"
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
