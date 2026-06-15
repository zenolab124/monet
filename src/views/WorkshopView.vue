<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import AssetList from '@/components/workshop/AssetList.vue'
import McpList from '@/components/workshop/McpList.vue'
import WorkshopNav from '@/components/workshop/WorkshopNav.vue'
import { useWorkshop, type WorkshopCategory } from '@/composables/useWorkshop'
import { useUiState } from '@/composables/useUiState'

/**
 * 工坊域（v2.3.0）：Skills / Commands / Subagents / MCP 四类资产只读陈列。
 * 左子导航 + 右列表，版式对齐 layout-lab.html 工坊区；无启停、无新建、无详情。
 */

const { t } = useI18n()
const { activeSection } = useUiState()
const { assets, loading, error, ensureLoaded, refresh, retry, probeStates, probeMcpServers } = useWorkshop()

/** 域内当前类别：本地状态，默认 Skills，不持久化（FR-002） */
const category = ref<WorkshopCategory>('skills')

/** 子导航配置（图标类名静态字面量，供 UnoCSS 扫描） */
const navItems: { key: WorkshopCategory; icon: string; label: string }[] = [
  { key: 'skills', icon: 'i-carbon-star', label: 'Skills' },
  { key: 'commands', icon: 'i-carbon-terminal', label: 'Commands' },
  { key: 'agents', icon: 'i-carbon-bot', label: 'Subagents' },
  { key: 'mcp', icon: 'i-carbon-plug', label: t('workshop.mcpServers') },
]

/** 页头文案（MCP 的「打开目录」改为「打开配置」，FR-006） */
const headConfig: Record<WorkshopCategory, { title: string; sub: string; openLabel: string }> = {
  skills: { title: 'Skills', sub: t('workshop.subSkills'), openLabel: t('workshop.openDir') },
  commands: { title: 'Commands', sub: t('workshop.subCommands'), openLabel: t('workshop.openDir') },
  agents: { title: 'Subagents', sub: t('workshop.subAgents'), openLabel: t('workshop.openDir') },
  mcp: { title: t('workshop.mcpServers'), sub: t('workshop.subMcp'), openLabel: t('common.openConfig') },
}
const head = computed(() => headConfig[category.value])

/** 子导航计数：数据未回显示「…」，加载失败显示「—」 */
const counts = computed<Record<WorkshopCategory, string>>(() => {
  const a = assets.value
  if (!a) {
    const placeholder = error.value ? '—' : '…'
    return { skills: placeholder, commands: placeholder, agents: placeholder, mcp: placeholder }
  }
  return {
    skills: String(a.skills.length),
    commands: String(a.commands.length),
    agents: String(a.agents.length),
    mcp: String(a.mcpServers.length),
  }
})

/** 当前类别的列表配置（mcp 为 null，走 McpList 分支） */
const currentList = computed(() => {
  const a = assets.value
  switch (category.value) {
    case 'skills':
      return { icon: 'i-carbon-star', items: a?.skills ?? [], empty: t('workshop.noSkills'), hint: t('workshop.hintSkills') }
    case 'commands':
      return { icon: 'i-carbon-terminal', items: a?.commands ?? [], empty: t('workshop.noCommands'), hint: t('workshop.hintCommands') }
    case 'agents':
      return { icon: 'i-carbon-bot', items: a?.agents ?? [], empty: t('workshop.noSubagents'), hint: t('workshop.hintSubagents') }
    case 'mcp':
      return null
  }
})

// 首次进入工坊惰性加载（含启动即恢复到工坊的场景）
watch(activeSection, (s) => {
  if (s === 'workshop') ensureLoaded()
}, { immediate: true })

// 每次进入 MCP 子页（域内切到 mcp，或带着 mcp 选中态切回工坊）重新探活
watch([activeSection, category], ([s, c]) => {
  if (s === 'workshop' && c === 'mcp') probeMcpServers()
})

// 数据到达/刷新成功时若停留在 MCP 子页，补发探活（覆盖首屏数据未回与刷新两条路径）
watch(assets, (a) => {
  if (a && activeSection.value === 'workshop' && category.value === 'mcp') probeMcpServers()
})

/** 「打开目录」失败的一次性文案，3 秒后消失（FR-006） */
const openFailed = ref(false)
let openFailTimer: ReturnType<typeof setTimeout> | undefined

async function openDir() {
  try {
    await invoke('open_workshop_dir', { category: category.value })
  } catch (_) {
    openFailed.value = true
    clearTimeout(openFailTimer)
    openFailTimer = setTimeout(() => { openFailed.value = false }, 3000)
  }
}
</script>

<template>
  <div class="h-full p-2.5">
    <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">
    <WorkshopNav v-model="category" :items="navItems" :counts="counts" />

    <main class="flex-1 min-w-0 overflow-y-auto px-6.5 py-5">
      <div class="content-area">
        <div class="flex items-center gap-2.5 mb-4">
          <h2 class="text-base font-semibold">{{ head.title }}</h2>
          <span class="text-xs text-muted-foreground">{{ head.sub }}</span>
          <div class="ml-auto flex items-center gap-1.5">
            <span v-if="openFailed" class="text-xs text-destructive">{{ $t('common.openFailed') }}</span>
            <button class="ws-btn" @click="openDir">{{ head.openLabel }}</button>
            <button class="ws-btn" :disabled="loading" @click="refresh">
              <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': loading }" />
              {{ $t('common.refresh') }}
            </button>
          </div>
        </div>

        <!-- 错误态：四个子导航仍可切换，壳不白屏（FR-002 错误路径） -->
        <div v-if="error" class="py-8 text-center">
          <p class="text-xs text-destructive">{{ $t('common.loadFailed') }}</p>
          <button class="ws-btn mt-3" @click="retry">{{ $t('common.retry') }}</button>
        </div>
        <!-- 加载态 -->
        <div v-else-if="!assets" class="py-8 text-center text-xs text-muted-foreground">
          {{ $t('common.loading') }}
        </div>
        <!-- 列表 -->
        <template v-else>
          <AssetList
            v-if="currentList"
            :items="currentList.items"
            :icon="currentList.icon"
            :empty-title="currentList.empty"
            :empty-hint="currentList.hint"
          />
          <McpList v-else :items="assets?.mcpServers ?? []" :probe-states="probeStates" />
        </template>
      </div>
    </main>
    </div>
  </div>
</template>

<style scoped>
.ws-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  padding: 3px 12px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--card);
  cursor: pointer;
}
.ws-btn:hover:not(:disabled) {
  box-shadow: var(--shadow-paper);
}
.ws-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
