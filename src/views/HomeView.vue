<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import HomeGrid from '../components/home/HomeGrid.vue'
import type { CardDef } from '../components/home/HomeGrid.vue'
import WidgetIframe from '../components/home/WidgetIframe.vue'
import TodaySummaryCard from '../components/home/TodaySummaryCard.vue'
import TokenCard from '../components/home/TokenCard.vue'
import CostEstimateCard from '../components/home/CostEstimateCard.vue'
import StreakCard from '../components/home/StreakCard.vue'
import ModelPreferenceCard from '../components/home/ModelPreferenceCard.vue'
import RecentSessionsCard from '../components/home/RecentSessionsCard.vue'
import ProjectActivityCard from '../components/home/ProjectActivityCard.vue'
import BranchActivityCard from '../components/home/BranchActivityCard.vue'
import WorkRhythmCard from '../components/home/WorkRhythmCard.vue'
import SessionDepthCard from '../components/home/SessionDepthCard.vue'
import HeatmapCard from '../components/home/HeatmapCard.vue'
import { useHomeStats } from '../composables/useHomeStats'
import { useProjects } from '../composables/useProjects'
import { useSessions } from '../composables/useSessions'
import { useUiState } from '../composables/useUiState'

const { t } = useI18n()
const { activeSection, switchSection } = useUiState()
const {
  usage, usageLoading, usageError, retryUsage,
  ensureLoaded,
} = useHomeStats()
const { projects, loading: projectsLoading, loadProjects } = useProjects()
const { selectSession } = useSessions()

interface DashboardWidget {
  id: string
  name: string
  description: string
  width: number
  height: number
  html: string
  createdAt: string
  updatedAt: string
}

const customWidgets = ref<Map<string, DashboardWidget>>(new Map())

watch(
  activeSection,
  (section) => {
    if (section === 'home') {
      ensureLoaded()
      loadProjects()
      loadCustomWidgets()
    }
  },
  { immediate: true },
)

watch(
  [usage, projects],
  ([u, p]) => {
    if (!u || !p.length) return
    const now = new Date()
    const todayDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
    const todayTokens = u.daily.find(d => d.date === todayDate)?.total ?? 0
    const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() / 1000
    let sessions = 0
    const models = new Set<string>()
    for (const proj of p) {
      for (const s of proj.sessions) {
        if (s.last_modified >= startOfDay) {
          sessions++
          if (s.model) models.add(s.model.replace(/^claude-/, ''))
        }
      }
    }
    invoke('update_widget', { todaySessions: sessions, todayTokens: todayTokens, models: [...models] }).catch(() => {})
  },
)

const builtinCards: CardDef[] = [
  { id: 'today',     x: 0, y: 0, w: 2, h: 1 },
  { id: 'streak',    x: 2, y: 0, w: 2, h: 1 },
  { id: 'token',     x: 0, y: 1, w: 2, h: 1 },
  { id: 'cost',      x: 2, y: 1, w: 2, h: 1 },
  { id: 'recent',    x: 0, y: 2, w: 4, h: 1 },
  { id: 'model',     x: 0, y: 3, w: 2, h: 1 },
  { id: 'project',   x: 2, y: 3, w: 2, h: 1 },
  { id: 'branch',    x: 0, y: 4, w: 2, h: 1 },
  { id: 'rhythm',    x: 2, y: 4, w: 2, h: 1 },
  { id: 'depth',     x: 0, y: 5, w: 2, h: 1 },
  { id: 'heatmap',   x: 0, y: 6, w: 4, h: 1 },
]

const allCards = computed<CardDef[]>(() => {
  const widgetCards: CardDef[] = []
  let nextY = 7
  for (const [, w] of customWidgets.value) {
    widgetCards.push({
      id: `w:${w.id}`,
      x: 0, y: nextY,
      w: w.width, h: w.height,
    })
    nextY += w.height
  }
  return [...builtinCards, ...widgetCards]
})

const gridRef = ref<InstanceType<typeof HomeGrid>>()

const headDate = computed(() => {
  const d = new Date()
  return t('time.dateHeader', { year: d.getFullYear(), month: d.getMonth() + 1, day: d.getDate() })
})

async function loadCustomWidgets() {
  try {
    const list = await invoke<DashboardWidget[]>('list_dashboard_widgets')
    const map = new Map<string, DashboardWidget>()
    for (const w of list) map.set(w.id, w)
    customWidgets.value = map
  } catch { /* ignore */ }
}

async function onDeleteWidget(gridId: string) {
  const widgetId = gridId.startsWith('w:') ? gridId.slice(2) : gridId
  try {
    await invoke('delete_dashboard_widget', { id: widgetId })
    customWidgets.value.delete(widgetId)
    customWidgets.value = new Map(customWidgets.value)
  } catch { /* ignore */ }
}

function onGoSession(sessionId: string) {
  selectSession(sessionId)
  switchSection('sessions')
}

function onSelectDate(date: string) {
  void date
  switchSection('sessions')
}
</script>

<template>
  <main class="h-full overflow-y-auto px-8 py-6.5">
    <div class="content-area">
      <div class="flex items-center gap-3 mb-4.5">
        <span class="text-xs text-muted-foreground">{{ headDate }}</span>
        <div class="ml-auto flex items-center gap-1.5">
          <button
            v-if="gridRef?.editing"
            class="grid-action-btn"
            :title="t('home.grid.reset')"
            @click="gridRef?.resetLayout()"
          >
            <span class="i-carbon-reset w-3.5 h-3.5" />
          </button>
          <button
            class="grid-action-btn"
            :class="{ active: gridRef?.editing }"
            :title="gridRef?.editing ? t('home.grid.done') : t('home.grid.edit')"
            @click="gridRef?.toggleEdit()"
          >
            <span class="w-3.5 h-3.5" :class="gridRef?.editing ? 'i-carbon-checkmark' : 'i-carbon-edit'" />
          </button>
        </div>
      </div>

      <HomeGrid ref="gridRef" :cards="allCards" @delete-widget="onDeleteWidget">
        <template #today>
          <TodaySummaryCard :usage="usage" :projects="projects" :loading="usageLoading || projectsLoading" />
        </template>
        <template #streak>
          <StreakCard :usage="usage" :loading="usageLoading" />
        </template>
        <template #token>
          <TokenCard :usage="usage" :loading="usageLoading" :error="usageError" @retry="retryUsage" />
        </template>
        <template #cost>
          <CostEstimateCard :projects="projects" :loading="projectsLoading" />
        </template>
        <template #recent>
          <RecentSessionsCard :projects="projects" :loading="projectsLoading" @go-session="onGoSession" />
        </template>
        <template #model>
          <ModelPreferenceCard :projects="projects" :loading="projectsLoading" />
        </template>
        <template #project>
          <ProjectActivityCard :projects="projects" :loading="projectsLoading" />
        </template>
        <template #branch>
          <BranchActivityCard :projects="projects" :loading="projectsLoading" />
        </template>
        <template #rhythm>
          <WorkRhythmCard :projects="projects" :loading="projectsLoading" />
        </template>
        <template #depth>
          <SessionDepthCard :projects="projects" :loading="projectsLoading" />
        </template>
        <template #heatmap>
          <HeatmapCard :usage="usage" :loading="usageLoading" :error="usageError" @retry="retryUsage" @select-date="onSelectDate" />
        </template>
        <template #custom-widget="{ card }">
          <WidgetIframe
            v-if="customWidgets.get(card.id.slice(2))"
            :name="customWidgets.get(card.id.slice(2))!.name"
            :html="customWidgets.get(card.id.slice(2))!.html"
          />
        </template>
      </HomeGrid>
    </div>
  </main>
</template>

<style scoped>
.grid-action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--muted-foreground);
  background: transparent;
  cursor: pointer;
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}
.grid-action-btn:hover {
  background: var(--muted);
  color: var(--foreground);
}
.grid-action-btn.active {
  border-color: var(--primary);
  color: var(--primary);
  background: color-mix(in srgb, var(--primary) 8%, transparent);
}
</style>
