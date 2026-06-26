<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  useChannels,
  refreshChannels,
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
const { channels, sessionChain } = useChannels()
const { switchSection } = useUiState()

/** 「跟随默认」在选项列表中的占位值(null 不能做 v-for key)。
 * 含冒号——validate_id 只允许 [a-zA-Z0-9_-],真实渠道 id 永不会等于它,杜绝冲突 */
const FOLLOW = ':follow:'

interface ChannelOption {
  value: string
  label: string
}

function chainFirstName(): string {
  for (const id of sessionChain.value) {
    if (id === OFFICIAL_CHANNEL_ID) return channelDisplayName(null)
    const ch = channels.value.find(c => c.id === id)
    if (ch?.enabled) return ch.name
  }
  return channelDisplayName(null)
}

const options = computed<ChannelOption[]>(() => {
  const result: ChannelOption[] = []
  for (const id of sessionChain.value) {
    if (id === OFFICIAL_CHANNEL_ID) {
      result.push({ value: OFFICIAL_CHANNEL_ID, label: t('topbar.channelOfficial') })
    } else {
      const ch = channels.value.find(c => c.id === id)
      if (ch?.enabled && ch.scope !== 'agent-only') result.push({ value: id, label: ch.name })
    }
  }
  return result
})

const resolvedChannel = computed(() => props.current ?? sessionChain.value[0] ?? OFFICIAL_CHANNEL_ID)

const currentIndex = computed(() =>
  options.value.findIndex(o => o.value === resolvedChannel.value),
)

const currentLabel = computed(() => {
  const id = resolvedChannel.value
  if (id === OFFICIAL_CHANNEL_ID) return t('topbar.channelOfficial')
  const ch = channels.value.find(c => c.id === id)
  return ch?.name ?? channelDisplayName(id)
})

const open = ref(false)
const containerRef = ref<HTMLElement>()
const buttonRef = ref<HTMLButtonElement>()
const focusedIndex = ref(0)

function toggle() {
  open.value = !open.value
  if (open.value) {
    // channels/*.json 是用户可手编的活文件:每次打开下拉重读,清单不显示过期值
    refreshChannels()
    focusedIndex.value = currentIndex.value >= 0 ? currentIndex.value : 0
    nextTick(() => focusListItem(focusedIndex.value))
  }
}

function close() {
  open.value = false
  buttonRef.value?.focus()
}

function selectAt(index: number) {
  const o = options.value[index]
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
  const len = options.value.length
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
      <span class="truncate max-w-24">{{ currentLabel }}</span>
      <span class="i-carbon-chevron-down w-3 h-3 text-muted-foreground" />
    </button>

    <ul
      v-if="open"
      role="listbox"
      class="absolute top-full left-0 mt-1 z-50 min-w-36 py-1 rounded-md border border-border
             shadow-paper-lifted bg-popover"
    >
      <li
        v-for="(o, i) in options"
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
