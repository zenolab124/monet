import { ref, computed, watch, onUnmounted, type WatchSource } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SubAgentMeta, SessionRecord } from '@/types'

export interface SubAgentState {
  meta: SubAgentMeta
  records: SessionRecord[]
  loading: boolean
}

export interface AsyncTask {
  id: string
  type: 'agent' | 'workflow'
  label: string
  description: string
  toolUseId: string
  children: SubAgentMeta[]
}

export function createSubAgentContext() {
  const subAgentMap = ref(new Map<string, SubAgentMeta>())
  const allAgents = ref<SubAgentMeta[]>([])
  const openAgents = ref<SubAgentState[]>([])
  const activeTabId = ref<string | null>(null)
  const sidebarOpen = ref(false)
  let currentProjectId = ''
  let currentSessionId = ''
  let pollTimer: ReturnType<typeof setInterval> | null = null

  const panelVisible = computed(() => sidebarOpen.value && allAgents.value.length > 0)
  const activeTab = computed(() =>
    openAgents.value.find(a => a.meta.agent_id === activeTabId.value) ?? null,
  )

  const asyncTasks = computed<AsyncTask[]>(() => {
    const agents = allAgents.value
    const workflowGroups = new Map<string, SubAgentMeta[]>()
    const directAgents: SubAgentMeta[] = []
    for (const a of agents) {
      if (a.workflow_id) {
        const list = workflowGroups.get(a.workflow_id) ?? []
        list.push(a)
        workflowGroups.set(a.workflow_id, list)
      } else {
        directAgents.push(a)
      }
    }
    const tasks: AsyncTask[] = []
    for (const a of directAgents) {
      tasks.push({
        id: a.agent_id,
        type: 'agent',
        label: a.agent_type ?? 'Agent',
        description: a.description ?? '',
        toolUseId: a.tool_use_id,
        children: [],
      })
    }
    for (const [wfId, children] of workflowGroups) {
      tasks.push({
        id: wfId,
        type: 'workflow',
        label: 'Workflow',
        description: `${children.length} agents`,
        toolUseId: '',
        children,
      })
    }
    return tasks
  })

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

  function startPolling(streamingRef: WatchSource<boolean>, unmatchedCheck: () => boolean) {
    const stopWatch = watch(streamingRef, (streaming) => {
      stopPolling()
      if (streaming && unmatchedCheck()) {
        pollTimer = setInterval(async () => {
          if (!unmatchedCheck()) {
            stopPolling()
            return
          }
          await loadSubAgentList(currentProjectId, currentSessionId)
          if (allAgents.value.length > 0 && !sidebarOpen.value) {
            sidebarOpen.value = true
          }
        }, 2000)
      }
    })

    onUnmounted(() => {
      stopPolling()
      stopWatch()
    })
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  return {
    subAgentMap,
    allAgents,
    openAgents,
    activeTabId,
    panelVisible,
    activeTab,
    asyncTasks,
    sidebarOpen,
    loadSubAgentList,
    findByToolUseId,
    toggleSubAgent,
    openSidebar,
    closeSidebar,
    closeTab,
    closeAllTabs,
    isAgentOpen,
    startPolling,
  }
}
