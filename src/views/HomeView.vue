<script setup lang="ts">
import { computed, watch } from 'vue'
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

const { activeSection, switchSection } = useUiState()
const {
  usage, usageLoading, usageError, retryUsage,
  ensureLoaded, refresh,
} = useHomeStats()
const { projects, loading: projectsLoading, loadProjects } = useProjects()
const { selectSession } = useSessions()

watch(
  activeSection,
  (section) => {
    if (section === 'home') {
      ensureLoaded()
      if (!projects.value.length) loadProjects()
    }
  },
  { immediate: true },
)

const refreshing = computed(() => usageLoading.value)

const headDate = computed(() => {
  void refreshing.value
  const d = new Date()
  return `${d.getFullYear()} 年 ${d.getMonth() + 1} 月 ${d.getDate()} 日 · 本月数据`
})

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
      <div class="flex items-baseline gap-3 mb-4.5">
        <span class="text-xs text-muted-foreground">{{ headDate }}</span>
      </div>

      <div class="card-grid">
        <TodaySummaryCard :usage="usage" :projects="projects" :loading="usageLoading || projectsLoading" />
        <StreakCard :usage="usage" :loading="usageLoading" />
        <TokenCard :usage="usage" :loading="usageLoading" :error="usageError" @retry="retryUsage" />
        <CostEstimateCard :projects="projects" :loading="projectsLoading" />
        <RecentSessionsCard :projects="projects" :loading="projectsLoading" @go-session="onGoSession" />
        <ModelPreferenceCard :projects="projects" :loading="projectsLoading" />
        <ProjectActivityCard :projects="projects" :loading="projectsLoading" />
        <BranchActivityCard :projects="projects" :loading="projectsLoading" />
        <WorkRhythmCard :projects="projects" :loading="projectsLoading" />
        <SessionDepthCard :projects="projects" :loading="projectsLoading" />
        <HeatmapCard :usage="usage" :loading="usageLoading" :error="usageError" @retry="retryUsage" @select-date="onSelectDate" />
      </div>
    </div>
  </main>
</template>

<style scoped>
.card-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
@media (max-width: 768px) {
  .card-grid {
    grid-template-columns: 1fr;
  }
}

.refresh-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--muted-foreground);
  background: transparent;
  cursor: pointer;
}
.refresh-btn:hover:not(:disabled) {
  background: var(--muted);
  color: var(--foreground);
}
.refresh-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
