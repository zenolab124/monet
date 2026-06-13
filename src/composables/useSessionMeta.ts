import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface SessionMeta {
  title?: string
  deleted?: boolean
  deletedAt?: string
  tags?: string[]
  starred?: boolean
}

const metaMap = ref<Record<string, SessionMeta>>({})
const titleGenerating = ref<Set<string>>(new Set())
let loaded = false

async function loadAll() {
  metaMap.value = await invoke<Record<string, SessionMeta>>('get_all_meta')
  loaded = true
}

function cwdToProjectId(cwd: string): string {
  return cwd.replace(/\//g, '-')
}

async function ensureTitle(projectId: string, sessionId: string) {
  if (metaMap.value[sessionId]?.title) return
  if (titleGenerating.value.has(sessionId)) return
  titleGenerating.value = new Set([...titleGenerating.value, sessionId])
  try {
    const title = await invoke<string>('generate_title', { projectId, sessionId })
    metaMap.value = { ...metaMap.value, [sessionId]: { ...metaMap.value[sessionId], title } }
  } catch (e) {
    console.warn('[meta] 标题生成失败:', sessionId, e)
  } finally {
    const next = new Set(titleGenerating.value)
    next.delete(sessionId)
    titleGenerating.value = next
  }
}

/** 用户发送消息后调用——异步生成标题，不阻塞发送流程 */
export function triggerTitleGeneration(sessionId: string, cwd: string) {
  ensureTitle(cwdToProjectId(cwd), sessionId)
}

export function useSessionMeta() {
  if (!loaded) loadAll()

  function getMeta(sessionId: string): SessionMeta | undefined {
    return metaMap.value[sessionId]
  }

  async function updateMeta(sessionId: string, patch: SessionMeta) {
    const updated = await invoke<SessionMeta>('update_meta', { sessionId, patch })
    metaMap.value = { ...metaMap.value, [sessionId]: updated }
    return updated
  }

  return { metaMap, getMeta, updateMeta, reloadMeta: loadAll, ensureTitle, titleGenerating }
}
