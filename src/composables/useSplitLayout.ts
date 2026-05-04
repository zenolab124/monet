import { ref } from 'vue'

/** 单个面板（叶节点） */
export interface PaneState {
  type: 'pane'
  id: string
  sessionId: string | null
}

/** 扁平 N 面板布局：所有 pane 横向平铺 */
export interface SplitState {
  panes: PaneState[]
  /** 各 pane 占整体宽度的比例，长度与 panes 一致，元素之和 = 1 */
  sizes: number[]
}

const STORAGE_KEY = 'cc-space-layout'

let idCounter = 0
function genId() {
  return `pane-${++idCounter}-${Date.now()}`
}

function createPane(sessionId: string | null = null): PaneState {
  return { type: 'pane', id: genId(), sessionId }
}

/** 等分 sizes：所有元素都为 1/n */
function equalSizes(n: number): number[] {
  return Array(n).fill(1 / n)
}

function createInitialState(): SplitState {
  return { panes: [createPane()], sizes: [1] }
}

// 初始布局：单面板（或从 localStorage 恢复）
const state = ref<SplitState>(loadLayout() || createInitialState())
const activePaneId = ref<string>(state.value.panes[0].id)

/** 在指定面板的右侧分屏；新 pane 自动获焦，所有 pane 重新等分 */
function splitPane(paneId: string, sessionId: string | null = null) {
  const idx = state.value.panes.findIndex(p => p.id === paneId)
  if (idx < 0) return
  const newPane = createPane(sessionId)
  const panes = [...state.value.panes]
  panes.splice(idx + 1, 0, newPane)
  state.value = { panes, sizes: equalSizes(panes.length) }
  activePaneId.value = newPane.id
  saveLayout()
}

/** 关闭面板（保留至少一个）；剩余 pane 重新等分 */
function closePane(paneId: string) {
  if (state.value.panes.length <= 1) return
  const idx = state.value.panes.findIndex(p => p.id === paneId)
  if (idx < 0) return
  const panes = state.value.panes.filter(p => p.id !== paneId)
  state.value = { panes, sizes: equalSizes(panes.length) }
  if (activePaneId.value === paneId) {
    activePaneId.value = panes[0].id
  }
  saveLayout()
}

/**
 * 拖动第 index 条分隔线（位于 panes[index] 与 panes[index+1] 之间）。
 * leftRatio 是 panes[index] 的目标新比例（占整体），仅在 combined 区间内重分配，
 * 其他 sizes 完全不动。clamp 防止把任一 pane 压没。
 */
function updateSize(index: number, leftRatio: number) {
  const sizes = state.value.sizes
  if (index < 0 || index >= sizes.length - 1) return
  const combined = sizes[index] + sizes[index + 1]
  const minLeft = 0.1 * combined
  const maxLeft = 0.9 * combined
  const clamped = Math.max(minLeft, Math.min(maxLeft, leftRatio))
  const next = [...sizes]
  next[index] = clamped
  next[index + 1] = combined - clamped
  state.value = { ...state.value, sizes: next }
  saveLayout()
}

/** 设置面板的会话 */
function setPaneSession(paneId: string, sessionId: string | null) {
  const pane = state.value.panes.find(p => p.id === paneId)
  if (pane) {
    pane.sessionId = sessionId
    saveLayout()
  }
}

/** 设置活跃面板 */
function setActivePane(paneId: string) {
  activePaneId.value = paneId
}

// --- 持久化 ---

function saveLayout() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state.value))
  } catch (_) {}
}

function loadLayout(): SplitState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw)
    if (!parsed || typeof parsed !== 'object') return null
    const { panes, sizes } = parsed as Partial<SplitState>
    // 基本结构校验：panes/sizes 长度一致、至少 1 个、sizes 之和接近 1
    if (!Array.isArray(panes) || !Array.isArray(sizes)) return null
    if (panes.length < 1 || sizes.length !== panes.length) return null
    for (const p of panes) {
      if (!p || p.type !== 'pane' || typeof p.id !== 'string') return null
    }
    const sum = sizes.reduce((a, b) => a + (typeof b === 'number' ? b : NaN), 0)
    if (!Number.isFinite(sum) || Math.abs(sum - 1) > 0.01) return null
    return { panes: panes as PaneState[], sizes: sizes as number[] }
  } catch (_) {
    return null
  }
}

export function useSplitLayout() {
  return {
    state,
    activePaneId,
    splitPane,
    closePane,
    updateSize,
    setPaneSession,
    setActivePane,
  }
}
