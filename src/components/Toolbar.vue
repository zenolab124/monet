<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'
import { useSessions } from '@/composables/useSessions'
import { useUiState } from '@/composables/useUiState'


const { searchQuery } = useSessions()
const { sidebarsCollapsed, toggleSidebars } = useUiState()

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
  <div
    class="h-11 shrink-0 flex items-center gap-3 pr-3 border-b border-border"
    data-tauri-drag-region
  >
    <div class="w-[30px] shrink-0" data-tauri-drag-region />

    <!-- 侧栏显隐切换 -->
    <button
      class="p-1.5 rounded-md hover:bg-muted transition-colors shrink-0"
      :title="sidebarsCollapsed ? $t('titlebar.toggleSidebar') : $t('titlebar.hideSidebar')"
      @click="toggleSidebars"
    >
      <span
        :class="[
          sidebarsCollapsed ? 'i-carbon-side-panel-open' : 'i-carbon-side-panel-close',
          'w-4 h-4 text-foreground block',
        ]"
      />
    </button>

    <!-- 搜索框 -->
    <div class="relative flex-1 max-w-xs">
      <span class="absolute left-2 top-1/2 -translate-y-1/2 i-carbon-search w-3.5 h-3.5 text-muted-foreground" />
      <input
        ref="inputRef"
        v-model="localQuery"
        type="text"
        :placeholder="$t('titlebar.searchAllSessions')"
        class="w-full pl-7 pr-7 py-1 text-xs rounded-md bg-popover border border-border
               text-foreground placeholder-muted-foreground
               focus:outline-none focus:border-ring transition-colors"
      />
      <button
        v-if="localQuery"
        class="absolute right-1.5 top-1/2 -translate-y-1/2 i-carbon-close w-3.5 h-3.5 text-muted-foreground hover:text-foreground"
        @click="clearSearch"
      />
    </div>

    <!-- 可拖拽区域（外观切换已移至 ActivityBar 底部） -->
    <div class="flex-1" data-tauri-drag-region />
  </div>
</template>
