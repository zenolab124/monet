import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Project, SessionSummary } from '@/types'
import { tokenTotal } from '@/types'

const projects = ref<Project[]>([])
const selectedProjectIds = ref<Set<string>>(new Set())
const loading = ref(false)
const error = ref<string | null>(null)
let watcherSetup = false

/** 加载所有项目 */
async function loadProjects() {
  loading.value = true
  error.value = null
  try {
    projects.value = await invoke<Project[]>('get_projects')
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }

  // 首次加载后注册文件监控监听
  if (!watcherSetup) {
    watcherSetup = true
    listen('projects-changed', () => {
      reloadProjectsSilently()
    })
  }
}

/** 静默重新加载（不显示 loading 状态） */
async function reloadProjectsSilently() {
  try {
    projects.value = await invoke<Project[]>('get_projects')
  } catch (_) {
    // 静默失败
  }
}

/** 切换项目选中状态 */
function toggleProject(id: string) {
  const s = new Set(selectedProjectIds.value)
  if (s.has(id)) {
    s.delete(id)
  } else {
    s.add(id)
  }
  selectedProjectIds.value = s
}

/** 全选/全不选 */
function selectAllProjects(select: boolean) {
  if (select) {
    selectedProjectIds.value = new Set(projects.value.map(p => p.id))
  } else {
    selectedProjectIds.value = new Set()
  }
}

/** 选中项目的会话（无选中时显示全部） */
const filteredSessions = computed<SessionSummary[]>(() => {
  const ids = selectedProjectIds.value
  const source = ids.size > 0
    ? projects.value.filter(p => ids.has(p.id))
    : projects.value
  return source.flatMap(p => p.sessions)
})

/** 侧边栏统计 */
const sidebarStats = computed(() => {
  const ps = projects.value
  const totalSessions = ps.reduce((sum, p) => sum + p.session_count, 0)
  const totalSize = ps.reduce(
    (sum, p) => sum + p.sessions.reduce((s, sess) => s + sess.file_size, 0),
    0,
  )
  return {
    projectCount: ps.length,
    sessionCount: totalSessions,
    totalSize,
  }
})

/** 会话列表统计（基于筛选后的会话） */
const sessionStats = computed(() => {
  const sessions = filteredSessions.value
  const totalTokens = sessions.reduce(
    (sum, s) => sum + tokenTotal(s.total_tokens),
    0,
  )
  const totalSize = sessions.reduce((sum, s) => sum + s.file_size, 0)
  // 活跃天数：去重日期
  const days = new Set(
    sessions
      .filter(s => s.last_modified)
      .map(s => new Date(s.last_modified * 1000).toDateString()),
  )
  return {
    sessionCount: sessions.length,
    totalTokens,
    totalSize,
    activeDays: days.size,
  }
})

export function useProjects() {
  return {
    projects,
    selectedProjectIds,
    loading,
    error,
    loadProjects,
    toggleProject,
    selectAllProjects,
    filteredSessions,
    sidebarStats,
    sessionStats,
  }
}
