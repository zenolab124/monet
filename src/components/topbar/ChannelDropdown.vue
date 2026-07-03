<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  useChannels,
  refreshChannels,
  resolveChannel,
  channelDisplayName,
  OFFICIAL_CHANNEL_ID,
} from '@/composables/useChannels'
import { useUiState } from '@/composables/useUiState'

const props = defineProps<{
  /** 会话渠道选择:null = 跟随应用默认;'official' = 强制官方;其他 = 渠道 id */
  current: string | null
}>()

const emit = defineEmits<{
  (e: 'select', channelId: string | null): void
}>()

const { t } = useI18n()
const { channels, defaultSessionChannel } = useChannels()
const { switchSection } = useUiState()

interface ChannelOption {
  value: string
  label: string
}

function chainFirstName(): string {
  if (defaultSessionChannel.value) {
    const ch = channels.value.find(c => c.id === defaultSessionChannel.value)
    if (ch?.enabled) return ch.name
  }
  return channelDisplayName(null)
}

const options = computed<ChannelOption[]>(() => {
  const result: ChannelOption[] = [
    { value: OFFICIAL_CHANNEL_ID, label: t('topbar.channelOfficial') },
  ]
  for (const ch of channels.value) {
    if (ch.id !== OFFICIAL_CHANNEL_ID && ch.enabled && ch.scope !== 'agent-only') {
      result.push({ value: ch.id, label: ch.name })
    }
  }
  return result
})

// 与发送链同一解析函数(含默认渠道禁用回落官方),按钮显示 = 实际注入
const resolvedChannel = computed(() => resolveChannel(props.current) ?? OFFICIAL_CHANNEL_ID)

/** 会话覆盖项在具体清单中的索引(打勾判定):跟随默认(null)时勾落在「默认」项 */
const currentIndex = computed(() =>
  props.current === null ? -1 : options.value.findIndex(o => o.value === props.current),
)

const currentLabel = computed(() => {
  const id = resolvedChannel.value
  if (id === OFFICIAL_CHANNEL_ID) return t('topbar.channelOfficial')
  const ch = channels.value.find(c => c.id === id)
  return ch?.name ?? channelDisplayName(id)
})

/** 跟随默认态:按钮弱化显示(值仍是解析后的实际渠道名) */
const inherited = computed(() => props.current === null)

/** 键盘导航总项数 = 「默认」项 + 具体渠道项 */
const totalCount = computed(() => options.value.length + 1)

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

function toggle() {
  open.value = !open.value
  if (open.value) {
    // channels/*.json 是用户可手编的活文件:每次打开下拉重读,清单不显示过期值
    refreshChannels()
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value + 1 : 0
    nextTick(() => focusListItem(focusedIndex.value))
  }
}

function close() {
  open.value = false
  buttonRef.value?.focus()
}

/** index 0 = 「默认」项(跟随应用默认渠道);1.. = 具体渠道项 */
function selectAt(index: number) {
  if (index === 0) {
    emit('select', null)
    close()
    return
  }
  const o = options.value[index - 1]
  if (!o) return
  emit('select', o.value)
  close()
}

function openSettings() {
  open.value = false
  switchSection('settings')
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return
  const len = totalCount.value
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value + 1) % len
      focusListItem(focusedIndex.value)
      break
    case 'ArrowUp':
      e.preventDefault()
      focusedIndex.value = (focusedIndex.value - 1 + len) % len
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
  // 挂载即拉一次:badge/标签首屏就能解析渠道名
  refreshChannels()
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
      :title="$t('topbar.channelTitle', { name: currentLabel })"
      :aria-haspopup="'listbox'"
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="truncate max-w-24" :class="{ 'opacity-70': inherited }">{{ currentLabel }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground" />
    </button>

    <ul
      v-if="open"
      role="listbox"
      class="absolute top-full left-0 mt-1 z-50 min-w-36 py-1 rounded-md border border-border
             shadow-paper-lifted bg-popover"
    >
      <!-- 「默认」项:跟随应用默认渠道(设置页切默认时随之变化) -->
      <li
        data-item
        role="option"
        tabindex="-1"
        :aria-selected="currentIndex < 0"
        :title="$t('topbar.inheritFromApp')"
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground focus:outline-none"
        @click="selectAt(0)"
        @mouseenter="focusedIndex = 0"
      >
        <span
          class="w-3 h-3 shrink-0"
          :class="currentIndex < 0 ? 'i-carbon-checkmark text-primary' : ''"
        />
        <span class="flex-1">{{ $t('topbar.channelDefault') }}</span>
        <span class="text-[10px] opacity-60 truncate max-w-20">{{ chainFirstName() }}</span>
      </li>

      <li
        v-for="(o, i) in options"
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
        <span class="flex-1 truncate">{{ o.label }}</span>
      </li>
      <li class="my-1 border-t border-border" role="separator" />
      <li
        class="px-2 py-1 text-xs flex items-center gap-2 cursor-pointer
               text-muted-foreground hover:bg-muted hover:text-foreground"
        @click="openSettings"
      >
        <span class="i-carbon-settings w-3 h-3 shrink-0" />
        <span class="flex-1">{{ $t('topbar.manageChannels') }}</span>
      </li>
    </ul>
  </div>
</template>
