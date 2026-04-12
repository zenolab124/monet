<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'
import { useSessions } from '@/composables/useSessions'
import { useTheme } from '@/composables/useTheme'

const { searchQuery } = useSessions()
const { mode, cycleTheme, themeLabel, themeIcon } = useTheme()

const inputRef = ref<HTMLInputElement>()
const localQuery = ref(searchQuery.value)

// 防抖 300ms
let debounceTimer: ReturnType<typeof setTimeout>
watch(localQuery, (v) => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    searchQuery.value = v
  }, 300)
})

// Cmd+F 聚焦
function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
    e.preventDefault()
    inputRef.value?.focus()
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

function clearSearch() {
  localQuery.value = ''
  searchQuery.value = ''
}
</script>

<template>
  <!-- macOS 红绿灯在左上角约 78px 宽，预留空间 -->
  <div
    class="h-11 shrink-0 flex items-center gap-3 pr-3 border-b border-divider bg-nav"
    data-tauri-drag-region
  >
    <!-- 红绿灯占位 -->
    <div class="w-[78px] shrink-0" data-tauri-drag-region />

    <!-- 搜索框 -->
    <div class="relative flex-1 max-w-xs">
      <span class="absolute left-2 top-1/2 -translate-y-1/2 i-carbon-search w-3.5 h-3.5 text-default4" />
      <input
        ref="inputRef"
        v-model="localQuery"
        type="text"
        placeholder="搜索全部会话… (⌘F)"
        class="w-full pl-7 pr-7 py-1 text-xs rounded-md bg-input border border-divider
               text-default placeholder-default4
               focus:outline-none focus:border-blue-500/50 transition-colors"
      />
      <button
        v-if="localQuery"
        class="absolute right-1.5 top-1/2 -translate-y-1/2 i-carbon-close w-3.5 h-3.5 text-default4 hover:text-default3"
        @click="clearSearch"
      />
    </div>

    <!-- 可拖拽区域 -->
    <div class="flex-1" data-tauri-drag-region />

    <!-- 外观切换 -->
    <button
      class="p-1.5 rounded-md hover:bg-hover transition-colors"
      :title="themeLabel[mode]"
      @click="cycleTheme"
    >
      <span :class="[themeIcon[mode], 'w-4 h-4 text-default3']" />
    </button>
  </div>
</template>
