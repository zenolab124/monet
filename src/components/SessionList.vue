<script setup lang="ts">
import { ref, computed } from 'vue'
import { useProjects } from '@/composables/useProjects'
import { useSessions, type SortOrder, type TimeRange } from '@/composables/useSessions'
import { useSplitLayout } from '@/composables/useSplitLayout'
import {
  displayTitle,
  relativeTime,
  formatTokens,
  formatBytes,
  tokenTotal,
  shortModel,
} from '@/types'
import type { SessionSummary } from '@/types'

const { filteredSessions, sessionStats, loadProjects } = useProjects()
const {
  selectedSessionId,
  sortOrder,
  selectedBranch,
  selectedTimeRange,
  selectedModel,
  filterAndSort,
  extractFilterOptions,
  selectSession,
} = useSessions()
const { activePaneId, splitPane, setPaneSession } = useSplitLayout()

const sortedSessions = computed(() => filterAndSort(filteredSessions.value))
const filterOptions = computed(() => extractFilterOptions(filteredSessions.value))

const sortLabels: Record<SortOrder, string> = {
  lastModified: '最近修改',
  tokenUsage: 'Token 消耗',
  messageCount: '消息数量',
}

const timeLabels: Record<TimeRange, string> = {
  all: '全部',
  today: '今天',
  thisWeek: '本周',
  thisMonth: '本月',
}

// 筛选下拉
const showBranchDropdown = ref(false)
const showModelDropdown = ref(false)

function pickBranch(branch: string) {
  selectedBranch.value = branch
  showBranchDropdown.value = false
}
function pickModel(model: string) {
  selectedModel.value = model
  showModelDropdown.value = false
}

// 分屏操作
function splitRight(session: SessionSummary) {
  splitPane(activePaneId.value, session.id)
}

// 原生右键菜单
import { invoke } from '@tauri-apps/api/core'
import { Menu } from '@tauri-apps/api/menu'
import { useWorkbench } from '@/composables/useWorkbench'
import { useConfirm } from '@/composables/useConfirm'

async function onContextMenu(e: MouseEvent, session: SessionSummary) {
  e.preventDefault()

  const items: Array<{ text: string; action: () => void }> = []

  if (session.cwd) {
    items.push({
      text: '在终端恢复',
      action: () => invoke('resume_in_terminal', { cwd: session.cwd!, sessionId: session.id }),
    })
  }

  items.push({
    text: '删除会话',
    action: async () => {
      const { projects } = useProjects()
      const project = projects.value.find(p => p.sessions.some(s => s.id === session.id))
      if (!project) return
      // 会话在工作台中:先确认(注明归属)再自动移出(FR-009)
      const { findSession, removeSession } = useWorkbench()
      const home = findSession(session.id)
      if (home) {
        const { confirm } = useConfirm()
        const ok = await confirm(
          `该会话在「${home.tab.name}」工作台中,删除将一并移出`,
          '删除',
        )
        if (!ok) return
        removeSession(session.id)
      }
      await invoke('delete_session', { projectId: project.id, sessionId: session.id })
      if (selectedSessionId.value === session.id) selectSession(null)
      loadProjects()
    },
  })

  const menu = await Menu.new({
    items: items.map(item => ({
      text: item.text,
      action: item.action,
    })),
  })
  await menu.popup()
}
</script>

<template>
  <div class="h-full flex flex-col">
    <!-- 统计卡片：横向铺开，数字与标签水平排列，避免窄宽度下换行 -->
    <div class="px-3 py-2 flex items-center gap-1.5 whitespace-nowrap overflow-hidden">
      <div class="flex-1 min-w-0 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground truncate">{{ sessionStats.sessionCount }}</span>
        <span class="text-xs text-muted-foreground shrink-0">会话</span>
      </div>
      <span class="w-px h-3 bg-divider shrink-0" />
      <div class="flex-1 min-w-0 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground truncate">{{ formatTokens(sessionStats.totalTokens) }}</span>
        <span class="text-xs text-muted-foreground shrink-0">Token</span>
      </div>
      <span class="w-px h-3 bg-divider shrink-0" />
      <div class="flex-1 min-w-0 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground truncate">{{ formatBytes(sessionStats.totalSize) }}</span>
        <span class="text-xs text-muted-foreground shrink-0">磁盘</span>
      </div>
      <span class="w-px h-3 bg-divider shrink-0" />
      <div class="flex-1 min-w-0 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground truncate">{{ sessionStats.activeDays }}</span>
        <span class="text-xs text-muted-foreground shrink-0">活跃</span>
      </div>
    </div>

    <!-- 排序 -->
    <div class="px-3 py-1 flex items-center gap-2">
      <button
        v-for="(label, key) in sortLabels"
        :key="key"
        class="px-2 py-0.5 text-xs rounded transition-colors"
        :class="sortOrder === key ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground'"
        @click="sortOrder = key as SortOrder"
      >
        {{ label }}
      </button>
      <span class="flex-1" />
      <button
        class="p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        title="刷新列表"
        @click="loadProjects"
      >
        <span class="i-carbon-renew w-3.5 h-3.5" />
      </button>
    </div>

    <!-- 筛选栏 -->
    <div class="px-3 py-1 flex flex-wrap gap-1.5 items-center">
      <!-- 时间范围 -->
      <button
        v-for="(label, key) in timeLabels"
        :key="key"
        class="px-2 py-0.5 text-xs rounded transition-colors"
        :class="selectedTimeRange === key ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground'"
        @click="selectedTimeRange = key as TimeRange"
      >
        {{ label }}
      </button>

      <span class="text-border">|</span>

      <!-- 分支下拉 -->
      <div class="relative">
        <button
          v-if="selectedBranch"
          class="px-2 py-0.5 text-xs rounded bg-secondary text-foreground flex items-center gap-1"
          @click="selectedBranch = null"
        >
          {{ selectedBranch }} ×
        </button>
        <button
          v-else
          class="px-2 py-0.5 text-xs rounded text-muted-foreground hover:text-foreground flex items-center gap-0.5"
          @click.stop="showBranchDropdown = !showBranchDropdown; showModelDropdown = false"
        >
          分支 <span class="i-carbon-chevron-down w-3 h-3" />
        </button>
        <div
          v-if="showBranchDropdown && filterOptions.branches.length"
          class="absolute top-full left-0 mt-1 z-10 bg-card border border-border rounded-md shadow-paper-lifted py-1 min-w-32 max-h-48 overflow-y-auto"
        >
          <button
            v-for="branch in filterOptions.branches"
            :key="branch"
            class="w-full text-left px-3 py-1 text-xs hover:bg-muted text-muted-foreground truncate"
            @click="pickBranch(branch)"
          >
            {{ branch }}
          </button>
        </div>
      </div>

      <!-- 模型下拉 -->
      <div class="relative">
        <button
          v-if="selectedModel"
          class="px-2 py-0.5 text-xs rounded bg-secondary text-foreground flex items-center gap-1"
          @click="selectedModel = null"
        >
          {{ shortModel(selectedModel) }} ×
        </button>
        <button
          v-else
          class="px-2 py-0.5 text-xs rounded text-muted-foreground hover:text-foreground flex items-center gap-0.5"
          @click.stop="showModelDropdown = !showModelDropdown; showBranchDropdown = false"
        >
          模型 <span class="i-carbon-chevron-down w-3 h-3" />
        </button>
        <div
          v-if="showModelDropdown && filterOptions.models.length"
          class="absolute top-full left-0 mt-1 z-10 bg-card border border-border rounded-md shadow-paper-lifted py-1 min-w-32 max-h-48 overflow-y-auto"
        >
          <button
            v-for="model in filterOptions.models"
            :key="model"
            class="w-full text-left px-3 py-1 text-xs hover:bg-muted text-muted-foreground truncate"
            @click="pickModel(model)"
          >
            {{ shortModel(model) }}
          </button>
        </div>
      </div>
    </div>

    <!-- 会话列表 -->
    <div class="flex-1 overflow-y-auto min-h-0 overscroll-y-contain p-2 flex flex-col gap-1">
      <div v-if="sortedSessions.length === 0" class="px-3 py-8 text-center">
        <p class="text-muted-foreground text-xs">没有找到会话</p>
        <p class="text-muted-foreground text-xs mt-1">尝试调整筛选条件</p>
      </div>

      <div
        v-for="session in sortedSessions"
        :key="session.id"
        class="w-full text-left px-3 py-2 rounded-md border border-transparent transition-colors hover:bg-muted cursor-pointer group relative shrink-0"
        :class="{ 'bg-card border-border shadow-paper': selectedSessionId === session.id }"
        @click="selectSession(session.id)"
        @contextmenu="onContextMenu($event, session)"
      >
        <div class="text-sm text-foreground truncate pr-12">
          {{ displayTitle(session) }}
        </div>
        <div class="text-xs text-muted-foreground mt-0.5 flex items-center gap-1.5 truncate">
          <span v-if="session.git_branch">{{ session.git_branch }}</span>
          <span v-if="session.git_branch">·</span>
          <span>{{ relativeTime(session.last_modified) }}</span>
          <span>·</span>
          <span>{{ formatTokens(tokenTotal(session.total_tokens)) }}</span>
          <span v-if="session.model">·</span>
          <span v-if="session.model" class="text-muted-foreground">{{ shortModel(session.model) }}</span>
        </div>

        <!-- 悬停分屏按钮 -->
        <div class="absolute right-2 top-1/2 -translate-y-1/2 hidden group-hover:flex gap-0.5">
          <button
            class="p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
            title="右侧分屏"
            @click.stop="splitRight(session)"
          >
            <span class="i-carbon-split-screen w-3 h-3" />
          </button>
        </div>
      </div>
    </div>

  </div>
</template>

