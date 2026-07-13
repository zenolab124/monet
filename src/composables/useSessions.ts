import { ref, computed, watch } from 'vue'
import { shortModel } from '@/types'
import type { SessionSummary } from '@/types'

export type SortOrder = 'lastModified' | 'tokenUsage' | 'messageCount'
export type TimeRange = 'all' | 'today' | 'thisWeek' | 'thisMonth'

const selectedSessionId = ref<string | null>(null)
const sortOrder = ref<SortOrder>('lastModified')
const selectedTimeRange = ref<TimeRange>('all')
const selectedModel = ref<string | null>(null)
/** 对会话列表应用筛选和排序 */
function filterAndSort(sessions: SessionSummary[]): SessionSummary[] {
  let result = sessions

  // 时间范围过滤
  if (selectedTimeRange.value !== 'all') {
    const now = Date.now()
    const cutoff = {
      today: now - 86400_000,
      thisWeek: now - 7 * 86400_000,
      thisMonth: now - 30 * 86400_000,
    }[selectedTimeRange.value]
    result = result.filter(s => s.last_modified * 1000 >= cutoff)
  }

  // 模型过滤（按 shortModel 匹配）
  if (selectedModel.value) {
    result = result.filter(s => shortModel(s.model) === selectedModel.value)
  }

  // 排序
  const sorters: Record<SortOrder, (a: SessionSummary, b: SessionSummary) => number> = {
    lastModified: (a, b) => b.last_modified - a.last_modified,
    tokenUsage: (a, b) => {
      const ta = a.total_tokens, tb = b.total_tokens
      return (tb.input_tokens + tb.output_tokens + tb.cache_creation_input_tokens + tb.cache_read_input_tokens)
        - (ta.input_tokens + ta.output_tokens + ta.cache_creation_input_tokens + ta.cache_read_input_tokens)
    },
    messageCount: (a, b) => b.message_count - a.message_count,
  }
  result = [...result].sort(sorters[sortOrder.value])

  return result
}

/** 从会话列表中提取可用的筛选选项 */
function extractFilterOptions(sessions: SessionSummary[]) {
  const models = new Set<string>()
  for (const s of sessions) {
    const m = shortModel(s.model)
    if (m) models.add(m)
  }
  return {
    models: [...models].sort(),
  }
}

function selectSession(id: string | null) {
  selectedSessionId.value = id
}

export function useSessions() {
  return {
    selectedSessionId,
    sortOrder,
    selectedTimeRange,
    selectedModel,
    filterAndSort,
    extractFilterOptions,
    selectSession,
  }
}
