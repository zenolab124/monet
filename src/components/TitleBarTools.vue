<script setup lang="ts">
import { ref, watch, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useUiState } from '@/composables/useUiState'
import { useSessions } from '@/composables/useSessions'
import { useHomeStats } from '@/composables/useHomeStats'
import { useAutomation } from '@/composables/useAutomation'

const { activeSection, sidebarsCollapsed, toggleSidebars } = useUiState()

// --- 档案馆搜索 ---
const { searchQuery } = useSessions()
const searchRef = ref<HTMLInputElement>()
const localQuery = ref(searchQuery.value)
let debounceTimer: ReturnType<typeof setTimeout>
watch(localQuery, (v) => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => { searchQuery.value = v }, 300)
})
function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === 'f' && activeSection.value === 'sessions') {
    e.preventDefault()
    searchRef.value?.focus()
  }
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

// --- 首页 ---
const { refresh: homeRefresh, usageLoading: homeRefreshing } = useHomeStats()

// --- 自动化 ---
const { config: autoConfig, refresh: autoRefresh, loadingConfig, loadingStats } = useAutomation()
const autoLoading = computed(() => loadingConfig.value || loadingStats.value)
const openFailMsg = ref<string | null>(null)
let openFailTimer: ReturnType<typeof setTimeout> | undefined
async function openGlobalConfig() {
  const home = autoConfig.value?.homePath ?? ''
  const path = `${home}/.claude/settings.json`
  openFailMsg.value = null
  try {
    await invoke('open_hooks_config', { path })
  } catch {
    openFailMsg.value = '打开失败'
    clearTimeout(openFailTimer)
    openFailTimer = setTimeout(() => { openFailMsg.value = null }, 3000)
  }
}
</script>

<template>
  <!-- 档案馆 -->
  <template v-if="activeSection === 'sessions'">
    <button
      class="p-1.5 rounded-md hover:bg-muted transition-colors"
      :title="sidebarsCollapsed ? '显示侧栏' : '隐藏侧栏'"
      @click="toggleSidebars"
    >
      <span
        :class="[sidebarsCollapsed ? 'i-carbon-side-panel-open' : 'i-carbon-side-panel-close', 'w-4 h-4 text-foreground block']"
      />
    </button>
    <div class="relative">
      <span class="absolute left-2 top-1/2 -translate-y-1/2 i-carbon-search w-3.5 h-3.5 text-muted-foreground" />
      <input
        ref="searchRef"
        v-model="localQuery"
        type="text"
        placeholder="搜索会话… (⌘F)"
        class="w-48 pl-7 pr-2 py-1 text-xs rounded-md bg-popover border border-border
               text-foreground placeholder-muted-foreground
               focus:outline-none focus:border-ring transition-colors"
      />
    </div>
  </template>

  <!-- 首页 -->
  <button v-if="activeSection === 'home'" class="tb-btn" :disabled="homeRefreshing" title="重新统计" @click="homeRefresh">
    <span class="i-carbon-renew w-3.5 h-3.5" :class="{ 'animate-spin': homeRefreshing }" />
  </button>

  <!-- 自动化 -->
  <template v-if="activeSection === 'automation'">
    <span v-if="openFailMsg" class="text-xs text-destructive">{{ openFailMsg }}</span>
    <button class="tb-btn" :disabled="!autoConfig" @click="openGlobalConfig">打开配置</button>
    <button class="tb-btn" :disabled="autoLoading" @click="autoRefresh">
      <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': autoLoading }" />
      刷新
    </button>
  </template>
</template>

<style scoped>
.tb-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  padding: 2px 8px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--card);
  cursor: pointer;
}
.tb-btn:hover:not(:disabled) {
  box-shadow: var(--shadow-paper);
}
.tb-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
