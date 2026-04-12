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
  splitPane(activePaneId.value, 'horizontal', session.id)
}
function splitDown(session: SessionSummary) {
  splitPane(activePaneId.value, 'vertical', session.id)
}

// 原生右键菜单
import { invoke } from '@tauri-apps/api/core'
import { Menu } from '@tauri-apps/api/menu'

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
    <!-- 统计卡片 -->
    <div class="px-3 py-2 grid grid-cols-4 gap-2">
      <div class="text-center">
        <div class="text-sm font-semibold text-default">{{ sessionStats.sessionCount }}</div>
        <div class="text-xs text-default4">会话</div>
      </div>
      <div class="text-center">
        <div class="text-sm font-semibold text-default">{{ formatTokens(sessionStats.totalTokens) }}</div>
        <div class="text-xs text-default4">Token</div>
      </div>
      <div class="text-center">
        <div class="text-sm font-semibold text-default">{{ formatBytes(sessionStats.totalSize) }}</div>
        <div class="text-xs text-default4">磁盘</div>
      </div>
      <div class="text-center">
        <div class="text-sm font-semibold text-default">{{ sessionStats.activeDays }}</div>
        <div class="text-xs text-default4">活跃</div>
      </div>
    </div>

    <!-- 排序 -->
    <div class="px-3 py-1 flex items-center gap-2">
      <button
        v-for="(label, key) in sortLabels"
        :key="key"
        class="px-2 py-0.5 text-xs rounded transition-colors"
        :class="sortOrder === key ? 'bg-active text-default' : 'text-default3 hover:text-default'"
        @click="sortOrder = key as SortOrder"
      >
        {{ label }}
      </button>
      <span class="flex-1" />
      <button
        class="p-1 rounded text-default4 hover:text-default3 hover:bg-hover transition-colors"
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
        :class="selectedTimeRange === key ? 'bg-blue-500/15 text-blue-400' : 'text-default4 hover:text-default3'"
        @click="selectedTimeRange = key as TimeRange"
      >
        {{ label }}
      </button>

      <span class="text-divider">|</span>

      <!-- 分支下拉 -->
      <div class="relative">
        <button
          v-if="selectedBranch"
          class="px-2 py-0.5 text-xs rounded bg-purple-500/15 text-purple-400 flex items-center gap-1"
          @click="selectedBranch = null"
        >
          {{ selectedBranch }} ×
        </button>
        <button
          v-else
          class="px-2 py-0.5 text-xs rounded text-default4 hover:text-default3 flex items-center gap-0.5"
          @click.stop="showBranchDropdown = !showBranchDropdown; showModelDropdown = false"
        >
          分支 <span class="i-carbon-chevron-down w-3 h-3" />
        </button>
        <div
          v-if="showBranchDropdown && filterOptions.branches.length"
          class="absolute top-full left-0 mt-1 z-10 bg-cardbg border border-divider rounded-md shadow-lg py-1 min-w-32 max-h-48 overflow-y-auto"
        >
          <button
            v-for="branch in filterOptions.branches"
            :key="branch"
            class="w-full text-left px-3 py-1 text-xs hover:bg-hover text-default3 truncate"
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
          class="px-2 py-0.5 text-xs rounded bg-green-500/15 text-green-400 flex items-center gap-1"
          @click="selectedModel = null"
        >
          {{ shortModel(selectedModel) }} ×
        </button>
        <button
          v-else
          class="px-2 py-0.5 text-xs rounded text-default4 hover:text-default3 flex items-center gap-0.5"
          @click.stop="showModelDropdown = !showModelDropdown; showBranchDropdown = false"
        >
          模型 <span class="i-carbon-chevron-down w-3 h-3" />
        </button>
        <div
          v-if="showModelDropdown && filterOptions.models.length"
          class="absolute top-full left-0 mt-1 z-10 bg-cardbg border border-divider rounded-md shadow-lg py-1 min-w-32 max-h-48 overflow-y-auto"
        >
          <button
            v-for="model in filterOptions.models"
            :key="model"
            class="w-full text-left px-3 py-1 text-xs hover:bg-hover text-default3 truncate"
            @click="pickModel(model)"
          >
            {{ shortModel(model) }}
          </button>
        </div>
      </div>
    </div>

    <!-- 会话列表 -->
    <div class="flex-1 overflow-y-auto min-h-0 overscroll-y-contain">
      <div v-if="sortedSessions.length === 0" class="px-3 py-8 text-center">
        <p class="text-default3 text-xs">没有找到会话</p>
        <p class="text-default4 text-xs mt-1">尝试调整筛选条件</p>
      </div>

      <div
        v-for="session in sortedSessions"
        :key="session.id"
        class="w-full text-left px-3 py-2 border-b border-divider/50 transition-colors hover:bg-hover cursor-pointer group relative"
        :class="{ 'bg-active': selectedSessionId === session.id }"
        @click="selectSession(session.id)"
        @contextmenu="onContextMenu($event, session)"
      >
        <div class="text-sm text-default truncate pr-12">
          {{ displayTitle(session) }}
        </div>
        <div class="text-xs text-default4 mt-0.5 flex items-center gap-1.5 truncate">
          <span v-if="session.git_branch" class="text-purple-400">{{ session.git_branch }}</span>
          <span v-if="session.git_branch">·</span>
          <span>{{ relativeTime(session.last_modified) }}</span>
          <span>·</span>
          <span>{{ formatTokens(tokenTotal(session.total_tokens)) }}</span>
          <span v-if="session.model">·</span>
          <span v-if="session.model" class="text-default3">{{ shortModel(session.model) }}</span>
        </div>

        <!-- 悬停分屏按钮 -->
        <div class="absolute right-2 top-1/2 -translate-y-1/2 hidden group-hover:flex gap-0.5">
          <button
            class="p-1 rounded text-default4 hover:text-default3 hover:bg-active transition-colors"
            title="右侧分屏"
            @click.stop="splitRight(session)"
          >
            <span class="i-carbon-split-screen w-3 h-3" />
          </button>
          <button
            class="p-1 rounded text-default4 hover:text-default3 hover:bg-active transition-colors"
            title="下方分屏"
            @click.stop="splitDown(session)"
          >
            <span class="i-carbon-row w-3 h-3" />
          </button>
        </div>
      </div>
    </div>

  </div>
</template>

