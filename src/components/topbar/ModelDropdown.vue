<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  inferModel,
  DEFAULT_CONTEXT,
  type ModelInfo,
} from '@/utils/modelContext'
import { useModelOptions } from '@/composables/useModelOptions'
import { useCliDefaults, refreshCliDefaults } from '@/composables/useCliDefaults'
import type { ResolvedRunConfig } from '@/composables/useRunConfig'

/**
 * 「显示与发送同源」:按钮展示 runConfig 解析出的下次发送模型(会话覆盖 >
 * 渠道默认 > CLI 默认),菜单首项「默认」= 清除会话覆盖回落继承链。
 * 历史真值(jsonl modelString)不再充当按钮兜底——那是"跑过什么",不是"将要用什么"。
 */
const props = defineProps<{
  /** 会话覆盖的模型 ID(null = 未覆盖,跟随渠道/CLI 默认) */
  current: string | null
  /** 运行配置解析结果(按钮显示 + 默认项预览 + 顾问锁定) */
  runConfig: ResolvedRunConfig
}>()

const { t } = useI18n()

const emit = defineEmits<{
  /** null = 清除会话覆盖(跟随渠道/CLI 默认) */
  (e: 'select', modelId: string | null): void
}>()

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

/** 顾问模式锁定主模型,下拉禁用 */
const disabled = computed(() => props.runConfig.modelSource === 'advisor')

const { cliDefaults } = useCliDefaults()

/** CLI 默认模型展示标签(settings.json model 字段可为别名,经清单解析;未收录显原名) */
const cliDefaultModelLabel = computed(() => {
  const m = cliDefaults.value.model
  if (!m) return null
  return inferModel(m)?.label ?? m
})

/** 渠道来源(按渠道产出候选:官方=角色主区+钉版本沉底;第三方=映射角色或回退全量) */
const channelRef = computed<string | null>(() => props.runConfig.channelId)
const { items: channelItems } = useModelOptions(channelRef)

/**
 * 具体模型项 = 渠道候选清单 + 当前覆盖的自定义模型(若清单外):
 * 会话在用清单外模型(CLI 自定义设置/代理模型)时附加为可选项,原名展示。
 */
const items = computed<ModelInfo[]>(() => {
  const base = channelItems.value
  if (props.current && !inferModel(props.current) && !base.some(m => m.id === props.current!.toLowerCase())) {
    return [
      ...base,
      { id: props.current, label: props.current, contextWindow: DEFAULT_CONTEXT },
    ]
  }
  return base
})

/** 把任意模型字符串解析为展示标签:渠道候选命中 > 清单推断 > 原名 */
function labelFor(modelStr: string): string {
  const hit = items.value.find(m => m.id === modelStr.toLowerCase())
  if (hit) return hit.label
  return inferModel(modelStr)?.label ?? modelStr
}

/** 菜单「默认」项的继承值预览:渠道默认 > CLI 默认 */
const inheritLabel = computed(() => {
  if (props.runConfig.channelDefaultModel) return labelFor(props.runConfig.channelDefaultModel)
  return cliDefaultModelLabel.value
})

/** 按钮标签 = 解析后的下次发送模型;CLI 层无读数时兜底「默认」字样 */
const currentLabel = computed(() => {
  const resolved = props.runConfig.model
  if (resolved) return labelFor(resolved)
  return cliDefaultModelLabel.value ?? t('topbar.modelDefault')
})

/** 继承态(非会话覆盖/顾问锁定):按钮弱化显示 + title 说明来源 */
const inherited = computed(
  () => props.runConfig.modelSource === 'channel' || props.runConfig.modelSource === 'cli',
)

const sourceTitle = computed(() => {
  if (disabled.value) return t('topbar.modelAdvisorLocked')
  const name = currentLabel.value
  switch (props.runConfig.modelSource) {
    case 'channel': return t('topbar.sourceChannel', { name })
    case 'cli': return t('topbar.sourceCli', { name })
    default: return t('topbar.modelTitle', { name })
  }
})

/** 会话覆盖项在具体清单中的索引(打勾判定):无覆盖时勾落在「默认」项 */
const currentIndex = computed(() => {
  const cur = props.current?.toLowerCase()
  if (!cur) return -1
  const exact = items.value.findIndex(m => m.id === cur)
  if (exact >= 0) return exact
  const inferred = inferModel(cur)
  return inferred ? items.value.findIndex(m => m.id === inferred.id) : -1
})

/** 键盘导航总项数 = 「默认」项 + 具体项 */
const totalCount = computed(() => items.value.length + 1)

function toggle() {
  if (disabled.value) return
  open.value = !open.value
  if (open.value) {
    // settings.json 是活文件:每次打开下拉重读,「默认」标签不显示过期值
    refreshCliDefaults()
    // 聚焦当前项:覆盖项(+1 偏移)或「默认」项
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value + 1 : 0
    nextTick(() => focusListItem(focusedIndex.value))
  }
}

function close() {
  open.value = false
  // 关闭后焦点回到触发按钮
  buttonRef.value?.focus()
}

/** index 0 = 「默认」项(清除覆盖);1.. = 具体模型项 */
function selectAt(index: number) {
  if (index === 0) {
    emit('select', null)
    close()
    return
  }
  const m = items.value[index - 1]
  if (!m) return
  emit('select', m.id)
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
      :disabled="disabled"
      class="px-1.5 py-0.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted
             transition-colors flex items-center gap-1 border border-border
             disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
      :title="sourceTitle"
      :aria-haspopup="'listbox'"
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="truncate max-w-32" :class="{ 'opacity-70': inherited }">{{ currentLabel }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground" />
    </button>

    <ul
      v-if="open"
      role="listbox"
      class="absolute top-full left-0 mt-1 z-50 min-w-36 py-1 rounded-md border border-border
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
        :title="runConfig.channelDefaultModel ? $t('topbar.inheritFromChannel') : $t('topbar.inheritFromCli')"
        @click="selectAt(0)"
        @mouseenter="focusedIndex = 0"
      >
        <span
          class="w-3 h-3 shrink-0"
          :class="currentIndex < 0 ? 'i-carbon-checkmark text-primary' : ''"
        />
        <span class="flex-1">{{ $t('topbar.modelDefault') }}</span>
        <span v-if="inheritLabel" class="text-[10px] opacity-60 truncate max-w-24">{{ inheritLabel }}</span>
      </li>

      <li
        v-for="(m, i) in items"
        :key="m.id"
        data-item
        role="option"
        tabindex="-1"
        :aria-selected="i === currentIndex"
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground focus:outline-none"
        :class="{ 'mt-1 pt-1.5 border-t border-border': i === 0 || (!!m.legacy !== !!items[i - 1].legacy) }"
        @click="selectAt(i + 1)"
        @mouseenter="focusedIndex = i + 1"
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
