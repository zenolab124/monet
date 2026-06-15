import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface SessionMeta {
  title?: string
  deleted?: boolean
  deletedAt?: string
  tags?: string[]
  starred?: boolean
  titleManual?: boolean
}

const metaMap = ref<Record<string, SessionMeta>>({})
const titleGenerating = ref<Set<string>>(new Set())
const turnCounts = new Map<string, number>()
let loaded = false

async function loadAll() {
  metaMap.value = await invoke<Record<string, SessionMeta>>('get_all_meta')
  loaded = true
}

function cwdToProjectId(cwd: string): string {
  return cwd.replace(/\//g, '-')
}

function shouldRefreshTitle(sessionId: string): boolean {
  const meta = metaMap.value[sessionId]
  if (meta?.titleManual) return false
  const turn = turnCounts.get(sessionId) ?? 1
  if (turn <= 5) return true
  return turn % 5 === 0
}

async function refreshTitle(projectId: string, sessionId: string) {
  if (!shouldRefreshTitle(sessionId)) return
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

/** 用户发送消息后调用——异步生成/修订标题，不阻塞发送流程 */
export function triggerTitleGeneration(sessionId: string, cwd: string) {
  const turn = (turnCounts.get(sessionId) ?? 0) + 1
  turnCounts.set(sessionId, turn)
  refreshTitle(cwdToProjectId(cwd), sessionId)
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
