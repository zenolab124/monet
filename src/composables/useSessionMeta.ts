import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { cwdToProjectId } from '@/utils/path'

export interface SessionMeta {
  title?: string
  deleted?: boolean
  deletedAt?: string
  tags?: string[]
  starred?: boolean
  titleManual?: boolean
  summary?: string
}

const metaMap = ref<Record<string, SessionMeta>>({})
const titleGenerating = ref<Set<string>>(new Set())
const turnCounts = new Map<string, number>()
let loaded = false

async function loadAll() {
  metaMap.value = await invoke<Record<string, SessionMeta>>('get_all_meta')
  loaded = true
}

function shouldRefresh(sessionId: string, manualKey?: keyof SessionMeta): boolean {
  const meta = metaMap.value[sessionId]
  if (manualKey && meta?.[manualKey]) return false
  const turn = turnCounts.get(sessionId) ?? 1
  if (turn <= 5) return true
  return turn % 5 === 0
}

async function refreshTitle(projectId: string, sessionId: string) {
  if (!shouldRefresh(sessionId, 'titleManual')) return
  if (titleGenerating.value.has(sessionId)) return
  titleGenerating.value = new Set([...titleGenerating.value, sessionId])
  try {
    const { title, turnCount } = await invoke<{ title: string, turnCount: number }>('generate_title', { projectId, sessionId })
    if (!turnCounts.has(sessionId)) {
      turnCounts.set(sessionId, turnCount)
    }
    metaMap.value = { ...metaMap.value, [sessionId]: { ...metaMap.value[sessionId], title } }
  } catch (e) {
    console.warn('[meta] 标题生成失败:', sessionId, e)
  } finally {
    const next = new Set(titleGenerating.value)
    next.delete(sessionId)
    titleGenerating.value = next
  }
}

async function refreshTags(projectId: string, sessionId: string) {
  if (!shouldRefresh(sessionId)) return
  try {
    const tags = await invoke<string[]>('generate_tags', { projectId, sessionId })
    metaMap.value = { ...metaMap.value, [sessionId]: { ...metaMap.value[sessionId], tags } }
  } catch (e) {
    console.warn('[meta] 标签生成失败:', sessionId, e)
  }
}

async function refreshSummary(projectId: string, sessionId: string) {
  if (!shouldRefresh(sessionId)) return
  try {
    const summary = await invoke<string>('generate_summary', { projectId, sessionId })
    metaMap.value = { ...metaMap.value, [sessionId]: { ...metaMap.value[sessionId], summary } }
  } catch (e) {
    console.warn('[meta] 摘要生成失败:', sessionId, e)
  }
}

/** 用户发送消息后调用——异步生成/修订标题、标签、摘要，不阻塞发送流程 */
export function triggerMetaGeneration(sessionId: string, cwd: string) {
  const turn = (turnCounts.get(sessionId) ?? 0) + 1
  turnCounts.set(sessionId, turn)
  const projectId = cwdToProjectId(cwd)
  refreshTitle(projectId, sessionId)
  refreshTags(projectId, sessionId)
  refreshSummary(projectId, sessionId)
}

export function useSessionMeta() {
  if (!loaded) loadAll()

  function getMeta(sessionId: string): SessionMeta | undefined {
    return metaMap.value[sessionId]
  }

  async function updateMeta(sessionId: string, patch: SessionMeta) {
    if (patch.title !== undefined) {
      patch.titleManual = true
    }
    const updated = await invoke<SessionMeta>('update_meta', { sessionId, patch })
    metaMap.value = { ...metaMap.value, [sessionId]: updated }
    return updated
  }

  return { metaMap, getMeta, updateMeta, reloadMeta: loadAll, refreshTitle, titleGenerating }
}
