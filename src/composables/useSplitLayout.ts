import { ref, watch } from 'vue'

export type SplitAxis = 'horizontal' | 'vertical'

export interface PaneState {
  type: 'pane'
  id: string
  sessionId: string | null
}

export interface SplitContainer {
  type: 'split'
  id: string
  axis: SplitAxis
  ratio: number
  first: SplitNode
  second: SplitNode
}

export type SplitNode = PaneState | SplitContainer

const STORAGE_KEY = 'cc-space-layout'

let idCounter = 0
function genId() {
  return `pane-${++idCounter}-${Date.now()}`
}

function createPane(sessionId: string | null = null): PaneState {
  return { type: 'pane', id: genId(), sessionId }
}

// 初始布局：单面板
const root = ref<SplitNode>(loadLayout() || createPane())
const activePaneId = ref<string>(findFirstPaneId(root.value))

/** 在指定面板旁边分屏 */
function splitPane(paneId: string, axis: SplitAxis, sessionId: string | null = null) {
  root.value = transformNode(root.value, paneId, (node) => {
    const newPane = createPane(sessionId)
    const container: SplitContainer = {
      type: 'split',
      id: genId(),
      axis,
      ratio: 0.5,
      first: node,
      second: newPane,
    }
    activePaneId.value = newPane.id
    return container
  })
  saveLayout()
}

/** 关闭面板（保留至少一个） */
function closePane(paneId: string) {
  const paneCount = countPanes(root.value)
  if (paneCount <= 1) return

  root.value = removeNode(root.value, paneId) || createPane()
  // 如果关闭的是活跃面板，切到第一个
  if (activePaneId.value === paneId) {
    activePaneId.value = findFirstPaneId(root.value)
  }
  saveLayout()
}

/** 更新分割比例 */
function updateRatio(splitId: string, ratio: number) {
  updateNodeRatio(root.value, splitId, Math.max(0.15, Math.min(0.85, ratio)))
  saveLayout()
}

/** 设置面板的会话 */
function setPaneSession(paneId: string, sessionId: string | null) {
  const pane = findPane(root.value, paneId)
  if (pane) {
    pane.sessionId = sessionId
    saveLayout()
  }
}

/** 设置活跃面板 */
function setActivePane(paneId: string) {
  activePaneId.value = paneId
}

// --- 树操作工具 ---

function transformNode(
  node: SplitNode,
  targetId: string,
  transform: (node: PaneState) => SplitNode,
): SplitNode {
  if (node.type === 'pane') {
    return node.id === targetId ? transform(node) : node
  }
  return {
    ...node,
    first: transformNode(node.first, targetId, transform),
    second: transformNode(node.second, targetId, transform),
  }
}

function removeNode(node: SplitNode, targetId: string): SplitNode | null {
  if (node.type === 'pane') {
    return node.id === targetId ? null : node
  }
  const first = removeNode(node.first, targetId)
  const second = removeNode(node.second, targetId)
  if (!first) return second
  if (!second) return first
  return { ...node, first, second }
}

function findPane(node: SplitNode, paneId: string): PaneState | null {
  if (node.type === 'pane') return node.id === paneId ? node : null
  return findPane(node.first, paneId) || findPane(node.second, paneId)
}

function findFirstPaneId(node: SplitNode): string {
  if (node.type === 'pane') return node.id
  return findFirstPaneId(node.first)
}

function countPanes(node: SplitNode): number {
  if (node.type === 'pane') return 1
  return countPanes(node.first) + countPanes(node.second)
}

function updateNodeRatio(node: SplitNode, splitId: string, ratio: number) {
  if (node.type === 'split') {
    if (node.id === splitId) {
      node.ratio = ratio
      return
    }
    updateNodeRatio(node.first, splitId, ratio)
    updateNodeRatio(node.second, splitId, ratio)
  }
}

// --- 持久化 ---

function saveLayout() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(root.value))
  } catch (_) {}
}

function loadLayout(): SplitNode | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    return JSON.parse(raw)
  } catch (_) {
    return null
  }
}

export function useSplitLayout() {
  return {
    root,
    activePaneId,
    splitPane,
    closePane,
    updateRatio,
    setPaneSession,
    setActivePane,
  }
}
