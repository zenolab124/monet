import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SubAgentMeta, SessionRecord } from '@/types'

// 子 Agent 转录管理：磁盘扫描（meta.json 清单）+ 详情 tab 打开/关闭。
// 任务发现与状态判定已移交异步账本（useAsyncTasks.buildAsyncLedger，从
// records 流实时推导）——这里只负责"可下钻的转录"。

export interface SubAgentState {
  meta: SubAgentMeta
  records: SessionRecord[]
  loading: boolean
}

export function createSubAgentContext() {
  const subAgentMap = ref(new Map<string, SubAgentMeta>())
  const allAgents = ref<SubAgentMeta[]>([])
  const openAgents = ref<SubAgentState[]>([])
  const activeTabId = ref<string | null>(null)
  const sidebarOpen = ref(false)
  let currentProjectId = ''
  let currentSessionId = ''

  const activeTab = computed(() =>
    openAgents.value.find(a => a.meta.agent_id === activeTabId.value) ?? null,
  )

  async function loadSubAgentList(projectId: string, sessionId: string) {
    currentProjectId = projectId
    currentSessionId = sessionId
    try {
      const list = await invoke<SubAgentMeta[]>('list_subagents', {
        projectId,
        sessionId,
      })
      allAgents.value = list
      const map = new Map<string, SubAgentMeta>()
      for (const item of list) {
        if (item.tool_use_id) map.set(item.tool_use_id, item)
      }
      subAgentMap.value = map
    } catch {
      allAgents.value = []
      subAgentMap.value = new Map()
    }
  }

  function findByToolUseId(toolUseId: string): SubAgentMeta | undefined {
    return subAgentMap.value.get(toolUseId)
  }

  function toggleSubAgent(meta: SubAgentMeta) {
    sidebarOpen.value = true
    const idx = openAgents.value.findIndex(a => a.meta.agent_id === meta.agent_id)
    if (idx >= 0) {
      activeTabId.value = meta.agent_id
      return
    }
    openTab(meta)
  }

  function openSidebar() {
    sidebarOpen.value = true
  }

  function closeSidebar() {
    sidebarOpen.value = false
  }

  async function openTab(meta: SubAgentMeta) {
    const state: SubAgentState = { meta, records: [], loading: true }
    openAgents.value = [...openAgents.value, state]
    activeTabId.value = meta.agent_id
    try {
      const records = await invoke<SessionRecord[]>('get_subagent_records', {
        projectId: currentProjectId,
        sessionId: currentSessionId,
        agentId: meta.agent_id,
      })
      openAgents.value = openAgents.value.map(a =>
        a.meta.agent_id === meta.agent_id ? { meta, records, loading: false } : a,
      )
    } catch {
      openAgents.value = openAgents.value.map(a =>
        a.meta.agent_id === meta.agent_id ? { meta, records: [], loading: false } : a,
      )
    }
  }

  function closeTab(agentId: string) {
    const remaining = openAgents.value.filter(a => a.meta.agent_id !== agentId)
    openAgents.value = remaining
    if (activeTabId.value === agentId) {
      activeTabId.value = remaining.length > 0 ? remaining[remaining.length - 1].meta.agent_id : null
    }
  }

  function closeAllTabs() {
    openAgents.value = []
    activeTabId.value = null
  }

  function isAgentOpen(agentId: string): boolean {
    return openAgents.value.some(a => a.meta.agent_id === agentId)
  }

  return {
    subAgentMap,
    allAgents,
    openAgents,
    activeTabId,
    activeTab,
    sidebarOpen,
    loadSubAgentList,
    findByToolUseId,
    toggleSubAgent,
    openSidebar,
    closeSidebar,
    closeTab,
    closeAllTabs,
    isAgentOpen,
  }
}
