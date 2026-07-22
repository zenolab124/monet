import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Project, SessionSummary } from '@/types'
import { tokenTotal } from '@/types'

const projects = ref<Project[]>([])
/** 数据修订号:全量与增量刷新后都 +1。增量路径原地 mutate 不换 projects 引用,
 *  浅层 watch(projects) 收不到,需要跨刷新方式感知变更的一律 watch 这个 */
const projectsRevision = ref(0)
const selectedProjectIds = ref<Set<string>>(new Set())
const loading = ref(false)
const error = ref<string | null>(null)
let watcherSetup = false

/** watcher 增量变更 payload（src-tauri/src/watcher.rs emit_pending_changes） */
interface SessionChange {
  projectId: string
  sessionId: string
}
interface ProjectsChangedPayload {
  full: boolean
  changes: SessionChange[]
}

/** 加载所有项目 */
async function loadProjects() {
  const hasCached = projects.value.length > 0
  if (!hasCached) loading.value = true
  error.value = null
  try {
    projects.value = await invoke<Project[]>('get_projects')
    projectsRevision.value++
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }

  // 首次加载后注册文件监控监听：按会话增量 patch，避免每秒全量整树替换
  // （docs/research/perf-audit-2026-07.md · P0-2）
  if (!watcherSetup) {
    watcherSetup = true
    listen<ProjectsChangedPayload>('projects-changed', (event) => {
      const payload = event.payload
      if (!payload || payload.full || !Array.isArray(payload.changes) || payload.changes.length === 0) {
        reloadProjectsSilently()
      } else {
        applySessionChanges(payload.changes)
      }
    })
  }
}

// 数据代际：每次增量变异 +1。全量拉取在途期间若有增量落地，
// 扫描结果可能比已落地的增量陈旧，检测到代际变化则重拉一次（无条件应用，避免循环）
let dataGen = 0

/** 静默重新加载（不显示 loading 状态） */
async function reloadProjectsSilently() {
  try {
    const genAtStart = dataGen
    const result = await invoke<Project[]>('get_projects')
    if (dataGen !== genAtStart) {
      projects.value = await invoke<Project[]>('get_projects')
    } else {
      projects.value = result
    }
    projectsRevision.value++
  } catch (_) {
    // 静默失败
  }
}

/** 按会话增量更新项目树；遇到未知项目（新建项目目录）回退全量 */
async function applySessionChanges(changes: SessionChange[]) {
  for (const { projectId, sessionId } of changes) {
    const proj = projects.value.find(p => p.id === projectId)
    if (!proj) {
      reloadProjectsSilently()
      return
    }
    try {
      const summary = await invoke<SessionSummary | null>('get_session_summary', {
        projectId,
        sessionId,
      })
      dataGen++
      const idx = proj.sessions.findIndex(s => s.id === sessionId)
      if (!summary) {
        // 会话文件已删除
        if (idx >= 0) proj.sessions.splice(idx, 1)
        if (proj.sessions.length === 0) {
          // 与全量扫描一致：零会话项目不展示
          const pIdx = projects.value.findIndex(p => p.id === projectId)
          if (pIdx >= 0) projects.value.splice(pIdx, 1)
          continue
        }
      } else if (idx >= 0) {
        proj.sessions[idx] = summary
      } else {
        proj.sessions.push(summary)
      }
      proj.session_count = proj.sessions.length
      proj.sessions.sort((a, b) => (b.last_modified ?? 0) - (a.last_modified ?? 0))
      proj.last_active = proj.sessions[0]?.last_modified ?? proj.last_active
    } catch (_) {
      // 单条失败不阻塞其余变更
    }
  }
  projects.value.sort((a, b) => (b.last_active ?? 0) - (a.last_active ?? 0))
  projectsRevision.value++
}

/** 切换项目选中状态(单选：点已选中的取消，点未选中的替换) */
function toggleProject(id: string) {
  selectedProjectIds.value = selectedProjectIds.value.has(id)
    ? new Set()
    : new Set([id])
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
    projectsRevision,
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
