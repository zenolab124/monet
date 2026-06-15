<script setup lang="ts">
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useProjects } from '@/composables/useProjects'
import { formatBytes } from '@/types'
import { relativeTime } from '@/types'

const { t } = useI18n()
const {
  projects,
  selectedProjectIds,
  loading,
  error,
  loadProjects,
  toggleProject,
  sidebarStats,
} = useProjects()

onMounted(() => {
  loadProjects()
})

function isSelected(id: string) {
  return selectedProjectIds.value.has(id)
}

/** 从完整路径中提取最后一段作为项目名 */
function projectName(displayPath: string) {
  const parts = displayPath.split('/')
  return parts[parts.length - 1] || displayPath
}
</script>

<template>
  <div class="h-full flex flex-col" @contextmenu.prevent>
    <!-- 标题 -->
    <div class="px-3 py-2 text-xs font-semibold text-muted-foreground tracking-wide">
      {{ $t('archive.projects') }}
    </div>

    <!-- 加载态 -->
    <div v-if="loading" class="px-3 py-8 text-center text-muted-foreground text-xs">
      {{ $t('archive.loadingProjects') }}
    </div>

    <!-- 错误态 -->
    <div v-else-if="error" class="px-3 py-4 text-xs text-destructive">
      {{ error }}
    </div>

    <!-- 空态 -->
    <div v-else-if="projects.length === 0" class="px-3 py-8 text-center">
      <p class="text-muted-foreground text-xs">{{ $t('archive.noProjects') }}</p>
      <p class="text-muted-foreground text-xs mt-1">{{ $t('archive.checkProjectsDir') }}</p>
    </div>

    <!-- 项目列表 -->
    <div v-else class="flex-1 overflow-y-auto min-h-0 overscroll-y-contain">
      <button
        v-for="project in projects"
        :key="project.id"
        class="w-full text-left px-3 py-1.5 flex items-start gap-2 rounded-md transition-colors
               hover:bg-muted group"
        :class="{ 'bg-card shadow-paper': isSelected(project.id) }"
        @click="toggleProject(project.id)"
      >
        <!-- 选中指示 -->
        <span
          class="w-1.5 h-1.5 rounded-full mt-1.5 shrink-0 transition-colors"
          :class="isSelected(project.id) ? 'bg-primary' : 'bg-transparent group-hover:bg-muted-foreground'"
        />
        <div class="min-w-0 flex-1">
          <div class="text-sm text-foreground truncate" :title="project.display_path">
            {{ projectName(project.display_path) }}
          </div>
          <div class="text-xs text-muted-foreground flex gap-2 mt-0.5">
            <span>{{ $t('archive.sessionCount', { count: project.session_count }) }}</span>
            <span v-if="project.last_active">{{ relativeTime(project.last_active) }}</span>
          </div>
        </div>
      </button>
    </div>

    <!-- 底部统计 -->
    <div class="px-3 py-2 border-t border-border text-xs text-muted-foreground flex gap-3">
      <span>{{ $t('archive.projectStats', { projects: sidebarStats.projectCount }) }}</span>
      <span>{{ $t('archive.sessionStats', { sessions: sidebarStats.sessionCount }) }}</span>
      <span>{{ formatBytes(sidebarStats.totalSize) }}</span>
    </div>
  </div>
</template>
