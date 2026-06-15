<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useProjects } from '@/composables/useProjects'
import { useSessions, type SortOrder, type TimeRange } from '@/composables/useSessions'
import { useSessionMeta } from '@/composables/useSessionMeta'
import {
  displayTitle,
  relativeTime,
  formatTokens,
  formatBytes,
  tokenTotal,
  shortModel,
} from '@/types'
import type { SessionSummary } from '@/types'

const { t } = useI18n()
const { getMeta } = useSessionMeta()

const summaryGenerating = ref<Set<string>>(new Set())

async function onGenerateSummary(e: MouseEvent, session: SessionSummary) {
  e.stopPropagation()
  if (summaryGenerating.value.has(session.id)) return
  const project = useProjects().projects.value.find(p => p.sessions.some(s => s.id === session.id))
  if (!project) return
  summaryGenerating.value = new Set([...summaryGenerating.value, session.id])
  try {
    const summary = await invoke<string>('generate_summary', { projectId: project.id, sessionId: session.id })
    const { updateMeta } = useSessionMeta()
    await updateMeta(session.id, { summary } as any)
  } catch (e) {
    console.warn('[meta] 摘要生成失败:', e)
  } finally {
    const next = new Set(summaryGenerating.value)
    next.delete(session.id)
    summaryGenerating.value = next
  }
}

const { filteredSessions, sessionStats, loadProjects } = useProjects()
const {
  selectedSessionId,
  sortOrder,
  selectedTimeRange,
  selectedModel,
  filterAndSort,
  extractFilterOptions,
  selectSession,
} = useSessions()
const sortedSessions = computed(() => filterAndSort(filteredSessions.value))
const filterOptions = computed(() => extractFilterOptions(filteredSessions.value))

const sortLabels = computed<Record<SortOrder, string>>(() => ({
  lastModified: t('archive.sortRecent'),
  tokenUsage: t('archive.sortTokens'),
  messageCount: t('archive.sortMessages'),
}))

const timeLabels = computed<Record<TimeRange, string>>(() => ({
  all: t('common.all'),
  today: t('archive.filterToday'),
  thisWeek: t('archive.filterWeek'),
  thisMonth: t('archive.filterMonth'),
}))

// 筛选下拉
const showModelDropdown = ref(false)

function pickModel(model: string) {
  selectedModel.value = model
  showModelDropdown.value = false
}

// 原生右键菜单
import { invoke } from '@tauri-apps/api/core'
import { Menu } from '@tauri-apps/api/menu'
import { useWorkbench } from '@/composables/useWorkbench'
import { useConfirm } from '@/composables/useConfirm'
import { readStoredChannelId } from '@/composables/useSessionSettings'
import { resolveChannel, refreshChannels } from '@/composables/useChannels'

async function onContextMenu(e: MouseEvent, session: SessionSummary) {
  e.preventDefault()

  const items: Array<{ text: string; action: () => void }> = []

  if (session.cwd) {
    items.push({
      text: t('archive.resumeInTerminal'),
      action: async () => {
        // 带上该会话已存的渠道(resolve 跟随默认),终端恢复不静默回落官方
        await refreshChannels()
        const channel = resolveChannel(readStoredChannelId(session.id))
        await invoke('resume_in_terminal', { cwd: session.cwd!, sessionId: session.id, channel })
      },
    })
  }

  items.push({
    text: t('archive.deleteSession'),
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
          t('archive.deleteSessionInWorkbench', { tabName: home.tab.name }),
          t('common.delete'),
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
    <!-- 统计卡片 -->
    <div class="px-3 py-2 flex items-center gap-1.5 whitespace-nowrap">
      <div class="flex-1 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground">{{ sessionStats.sessionCount }}</span>
        <span class="text-xs text-muted-foreground">{{ $t('archive.sessionLabel') }}</span>
      </div>
      <span class="w-px h-3 bg-divider shrink-0" />
      <div class="flex-1 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground">{{ formatTokens(sessionStats.totalTokens) }}</span>
        <span class="text-xs text-muted-foreground">Token</span>
      </div>
      <span class="w-px h-3 bg-divider shrink-0" />
      <div class="flex-1 flex items-baseline gap-1 justify-center">
        <span class="text-sm font-semibold text-foreground">{{ formatBytes(sessionStats.totalSize) }}</span>
        <span class="text-xs text-muted-foreground">{{ $t('archive.diskLabel') }}</span>
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
        :title="$t('archive.refreshList')"
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

      <!-- 模型下拉 -->
      <div class="relative">
        <button
          v-if="selectedModel"
          class="px-2 py-0.5 text-xs rounded bg-secondary text-foreground flex items-center gap-1"
          @click="selectedModel = null"
        >
          {{ selectedModel }} ×
        </button>
        <button
          v-else
          class="px-2 py-0.5 text-xs rounded text-muted-foreground hover:text-foreground flex items-center gap-0.5"
          @click.stop="showModelDropdown = !showModelDropdown"
        >
          {{ $t('archive.filterModel') }} <span class="i-carbon-chevron-down w-3 h-3" />
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
            {{ model }}
          </button>
        </div>
      </div>
    </div>

    <!-- 会话列表 -->
    <div class="flex-1 overflow-y-auto min-h-0 overscroll-y-contain p-2 flex flex-col gap-1">
      <div v-if="sortedSessions.length === 0" class="px-3 py-8 text-center">
        <p class="text-muted-foreground text-xs">{{ $t('archive.noSessions') }}</p>
        <p class="text-muted-foreground text-xs mt-1">{{ $t('archive.adjustFilter') }}</p>
      </div>

      <template
        v-for="(session, i) in sortedSessions"
        :key="session.id"
      >
      <div v-if="i > 0" class="mx-3 border-t border-border/30" />
      <div
        class="w-full text-left px-3 py-2 rounded-md border border-transparent transition-colors cursor-pointer group relative shrink-0"
        :class="selectedSessionId === session.id ? 'bg-card border-border shadow-paper' : 'hover:bg-muted'"
        @click="selectSession(session.id)"
        @contextmenu="onContextMenu($event, session)"
      >
        <div class="text-sm text-foreground truncate">
          {{ displayTitle(session, getMeta(session.id)?.title) }}
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
        <!-- 摘要 -->
        <div v-if="getMeta(session.id)?.summary" class="text-[11px] text-muted-foreground/70 mt-1 line-clamp-2 leading-relaxed">
          {{ getMeta(session.id)!.summary }}
        </div>
        <!-- 生成摘要按钮 -->
        <button
          v-else
          class="mt-1 text-[11px] text-muted-foreground/50 hover:text-primary opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1"
          @click="onGenerateSummary($event, session)"
        >
          <span v-if="summaryGenerating.has(session.id)" class="i-carbon-renew w-3 h-3 animate-spin" />
          <span v-else class="i-carbon-text-short-paragraph w-3 h-3" />
          {{ summaryGenerating.has(session.id) ? $t('archive.generatingSummary') : $t('archive.generateSummary') }}
        </button>
      </div>
      </template>
    </div>

  </div>
</template>

