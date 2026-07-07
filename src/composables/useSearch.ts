import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSessions } from './useSessions'
import { useUiState } from './useUiState'

/** 全局搜索状态（模块级单例，同 useHomeStats 模式）*/

export interface SearchSnippet {
  uuid: string | null
  role: number // 0 = user, 1 = assistant
  timestamp: string | null
  text: string
}

export interface SearchHit {
  sessionId: string
  projectId: string
  title: string | null
  lastModified: number
  matchedIn: string[]
  totalMatches: number
  snippets: SearchSnippet[]
}

export interface SearchResult {
  hits: SearchHit[]
  totalHits: number
  elapsedMs: number
}

export interface SearchStatus {
  state: 'building' | 'ready'
  indexedSessions: number
  totalSessions: number
}

const DEBOUNCE_MS = 300

const query = ref('')
const days30 = ref(false)
const titleOnly = ref(false)
const projectFilter = ref<string | null>(null)

const result = ref<SearchResult | null>(null)
const searching = ref(false)
const searchError = ref<string | null>(null)
const indexStatus = ref<SearchStatus | null>(null)

/** 跳转档案馆后待定位的命中消息（SessionDetail 消费后置 null）*/
const pendingScrollTarget = ref<{ sessionId: string; uuid: string } | null>(null)

let debounceTimer: ReturnType<typeof setTimeout> | null = null
let seq = 0

async function runSearch() {
  const q = query.value.trim()
  if (!q) {
    result.value = null
    searching.value = false
    return
  }
  const mySeq = ++seq
  searching.value = true
  searchError.value = null
  try {
    const r = await invoke<SearchResult>('search_query', {
      query: q,
      filter: {
        projectId: projectFilter.value,
        days: days30.value ? 30 : null,
        titleOnly: titleOnly.value,
      },
    })
    if (mySeq !== seq) return // 竞态：只接受最新请求
    result.value = r
    // query 内部懒热(首查即首建),搜完顺手刷状态让"构建中"标签自愈为就绪
    if (indexStatus.value?.state !== 'ready') refreshStatus()
  } catch (e) {
    if (mySeq !== seq) return
    searchError.value = String(e)
  } finally {
    if (mySeq === seq) searching.value = false
  }
}

// as-you-type：查询词防抖；过滤器变化即时
watch(query, () => {
  if (debounceTimer) clearTimeout(debounceTimer)
  debounceTimer = setTimeout(runSearch, DEBOUNCE_MS)
})
watch([days30, titleOnly, projectFilter], runSearch)

async function refreshStatus() {
  try {
    indexStatus.value = await invoke<SearchStatus>('search_status')
  } catch (_) {}
}

/** 结果卡点击：跳档案馆打开会话，可选定位到命中消息 */
function goToHit(hit: SearchHit, uuid?: string | null) {
  const { selectSession } = useSessions()
  const { switchSection } = useUiState()
  pendingScrollTarget.value = uuid ? { sessionId: hit.sessionId, uuid } : null
  selectSession(hit.sessionId)
  switchSection('sessions')
}

export function useSearch() {
  return {
    query,
    days30,
    titleOnly,
    projectFilter,
    result,
    searching,
    searchError,
    indexStatus,
    pendingScrollTarget,
    runSearch,
    refreshStatus,
    goToHit,
  }
}
