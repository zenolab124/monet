import { ref, computed, watch } from 'vue'

/**
 * 工作台状态模型（v2.1.0 FR-001/002/004 + NFR-002）
 *
 * 核心心智:在工作台 = 激活,与运行状态无关;会话进出全显式。
 * 列与会话解耦（对比组口子,硬约束）:列对象引用会话 id 并预留 type 字段,
 * 本版唯一取值 'session',禁止把列实现为会话对象本身的属性。
 * 会话归属唯一:一个会话同一时刻至多属于一个工作台 tab。
 */

export interface WorkbenchColumn {
  id: string
  type: 'session'
  sessionId: string
  /** 展开序号(全局递增):软上限置换时收回最小者 */
  openedSeq: number
}

export interface WorkbenchTab {
  id: string
  name: string
  /** 左列会话,加入顺序即显示顺序(任何状态变化不重排) */
  sessionIds: string[]
  /** 右区展开列(数组序 = 列序,可拖拽重排) */
  columns: WorkbenchColumn[]
  /** 各列宽度比例,与 columns 平行,和 ≈ 1 */
  columnSizes: number[]
}

interface WorkbenchState {
  tabs: WorkbenchTab[]
  activeTabId: string
  /** 「工作台 N」的 N:历史递增,关闭 tab 不回收 */
  tabSeq: number
  /** 展开序号计数 */
  openSeq: number
  /**
   * 应用内新建、尚未落盘的草稿会话(sessionId → cwd)。
   * 首条消息经 CLI --session-id 落盘后由 pruneDrafts 清理;
   * 落盘前各视图据此合成「新会话」占位显示。
   */
  drafts: Record<string, string>
}

const STORAGE_KEY = 'cc-space-workbench'

/** 单列最小可用宽度(px):低于此值会话可读性崩坏(顶栏全折叠、输入框过窄) */
export const MIN_COLUMN_WIDTH = 360

/** 右区四周边距与列间隙(与 WorkbenchColumns 的 PAD/GAP 一致,动态上限计算用) */
const COLUMN_GAP = 10

/**
 * 右区容器实测宽度:WorkbenchColumns 挂载后经 ResizeObserver 维护。
 * v-show 隐藏报 0 不更新(保留最后有效值);初始按窗口减 ActivityBar(48)+左列(256) 估算。
 */
const rightZoneWidth = ref(Math.max(MIN_COLUMN_WIDTH, window.innerWidth - 48 - 256))

export function setRightZoneWidth(w: number) {
  if (w > 0) rightZoneWidth.value = w
}

/**
 * 动态列上限(FR-004 演进):不固定列数,按「每列 ≥ 最小宽度」推算容量——
 * n·MIN + (n-1)·GAP + 2·PAD ≤ W。屏幕够宽就能开更多列,窄屏自动收紧;
 * 到达上限仍是软置换(最早展开的收回左列),不是拒绝。
 */
function dynamicMaxColumns(): number {
  return Math.max(
    1,
    Math.floor((rightZoneWidth.value - COLUMN_GAP) / (MIN_COLUMN_WIDTH + COLUMN_GAP)),
  )
}

/**
 * 容量收紧(窗口缩小后调用):全部 tab 中超出动态上限的列收回左列,
 * 最早展开者(openedSeq 最小)优先,与展开置换同序。
 * @returns 当前激活 tab 被收回的会话(调用方 toast 提示;非激活 tab 静默收)
 */
function enforceColumnCapacity(): string[] {
  const max = dynamicMaxColumns()
  const collapsedInActive: string[] = []
  for (const tab of state.value.tabs) {
    let changed = false
    while (tab.columns.length > max) {
      const earliest = tab.columns.reduce((min, c) => (c.openedSeq < min.openedSeq ? c : min))
      const idx = tab.columns.findIndex(c => c.id === earliest.id)
      tab.columns.splice(idx, 1)
      changed = true
      if (tab.id === state.value.activeTabId) collapsedInActive.push(earliest.sessionId)
    }
    if (changed) tab.columnSizes = equalSizes(tab.columns.length)
  }
  return collapsedInActive
}

let idCounter = 0
function genId(prefix: string) {
  return `${prefix}-${++idCounter}-${Date.now().toString(36)}`
}

function equalSizes(n: number): number[] {
  return n <= 0 ? [] : Array(n).fill(1 / n)
}

function createTabObject(seq: number): WorkbenchTab {
  return {
    id: genId('wbtab'),
    name: `工作台 ${seq}`,
    sessionIds: [],
    columns: [],
    columnSizes: [],
  }
}

function createInitialState(): WorkbenchState {
  const tab = createTabObject(1)
  return { tabs: [tab], activeTabId: tab.id, tabSeq: 1, openSeq: 0, drafts: {} }
}

// ---- 持久化(NFR-002):任一变更后同步落盘;损坏时回退默认并提示 ----

function saveState() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state.value))
  } catch (_) {}
}

/** 严格校验反序列化结果,任一处不符即整体作废 */
function loadState(): WorkbenchState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<WorkbenchState>
    if (!parsed || typeof parsed !== 'object') return null
    if (!Array.isArray(parsed.tabs) || parsed.tabs.length < 1) return null
    if (typeof parsed.activeTabId !== 'string') return null
    for (const t of parsed.tabs) {
      if (!t || typeof t.id !== 'string' || typeof t.name !== 'string') return null
      if (!Array.isArray(t.sessionIds) || !Array.isArray(t.columns) || !Array.isArray(t.columnSizes)) return null
      if (t.columns.length !== t.columnSizes.length) return null
      for (const sid of t.sessionIds) {
        if (typeof sid !== 'string') return null
      }
      for (const c of t.columns) {
        if (!c || c.type !== 'session' || typeof c.id !== 'string') return null
        if (typeof c.sessionId !== 'string' || typeof c.openedSeq !== 'number') return null
        // 列引用的会话必须在左列中
        if (!t.sessionIds.includes(c.sessionId)) return null
      }
      if (t.columnSizes.some(s => typeof s !== 'number' || !Number.isFinite(s))) return null
      if (t.columns.length > 0) {
        const sum = t.columnSizes.reduce((a, b) => a + b, 0)
        if (Math.abs(sum - 1) > 0.01) return null
      }
    }
    if (!parsed.tabs.some(t => t.id === parsed.activeTabId)) return null
    // drafts 为 v2.1.x 增量字段:旧数据缺省为 {},值非法则丢弃单条不作废整体
    const drafts: Record<string, string> = {}
    if (parsed.drafts && typeof parsed.drafts === 'object' && !Array.isArray(parsed.drafts)) {
      for (const [k, v] of Object.entries(parsed.drafts)) {
        if (typeof v === 'string' && v) drafts[k] = v
      }
    }
    return {
      tabs: parsed.tabs as WorkbenchTab[],
      activeTabId: parsed.activeTabId,
      tabSeq: typeof parsed.tabSeq === 'number' ? parsed.tabSeq : parsed.tabs.length,
      openSeq: typeof parsed.openSeq === 'number' ? parsed.openSeq : 0,
      drafts,
    }
  } catch (_) {
    return null
  }
}

const loaded = loadState()

/** 持久化损坏被重置(App 启动后弹瞬态 toast「工作台状态已重置」) */
export const stateWasReset = !!localStorage.getItem(STORAGE_KEY) && !loaded

const state = ref<WorkbenchState>(loaded || createInitialState())

watch(state, saveState, { deep: true })

// ---- 派生 ----

const activeTab = computed<WorkbenchTab>(() => {
  return state.value.tabs.find(t => t.id === state.value.activeTabId) ?? state.value.tabs[0]
})

/** 重复打开时的高亮目标(背景闪烁 1 秒) */
const flashSessionId = ref<string | null>(null)
let flashTimer: number | null = null

function flashSession(sessionId: string) {
  flashSessionId.value = sessionId
  if (flashTimer !== null) clearTimeout(flashTimer)
  flashTimer = window.setTimeout(() => {
    flashSessionId.value = null
    flashTimer = null
  }, 1000)
}

/** 右区滚动聚焦请求(已展开列的幂等展开;消费方为 WorkbenchColumns) */
const focusColumnRequest = ref<{ sessionId: string; seq: number } | null>(null)
let focusSeq = 0

function requestFocusColumn(sessionId: string) {
  focusColumnRequest.value = { sessionId, seq: ++focusSeq }
}

// ---- 查询 ----

/** 查会话归属(唯一性):返回所在 tab 与是否已展开 */
function findSession(sessionId: string): { tab: WorkbenchTab; expanded: boolean } | null {
  for (const tab of state.value.tabs) {
    if (tab.sessionIds.includes(sessionId)) {
      return { tab, expanded: tab.columns.some(c => c.sessionId === sessionId) }
    }
  }
  return null
}

/** 会话是否在「当前激活 tab 的展开列」中(完成通知的可见性判定,FR-006) */
function isSessionVisibleInWorkbench(sessionId: string): boolean {
  return activeTab.value.columns.some(c => c.sessionId === sessionId)
}

// ---- tab 操作(FR-001) ----

function createTab(): WorkbenchTab {
  state.value.tabSeq += 1
  const tab = createTabObject(state.value.tabSeq)
  state.value.tabs.push(tab)
  state.value.activeTabId = tab.id
  return tab
}

/** 重命名:1–20 字符,超长截断,空名回退原名 */
function renameTab(tabId: string, name: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const trimmed = name.trim().slice(0, 20)
  if (trimmed) tab.name = trimmed
}

/** 关闭 tab(连带清退其中全部会话)。最后一个 tab 不可关。调用方负责确认弹窗 */
function closeTab(tabId: string) {
  if (state.value.tabs.length <= 1) return
  const idx = state.value.tabs.findIndex(t => t.id === tabId)
  if (idx < 0) return
  state.value.tabs.splice(idx, 1)
  if (state.value.activeTabId === tabId) {
    state.value.activeTabId = state.value.tabs[Math.max(0, idx - 1)].id
  }
}

function setActiveTab(tabId: string) {
  if (state.value.tabs.some(t => t.id === tabId)) {
    state.value.activeTabId = tabId
  }
}

function reorderTabs(fromIndex: number, toIndex: number) {
  const tabs = state.value.tabs
  if (fromIndex < 0 || fromIndex >= tabs.length || toIndex < 0 || toIndex >= tabs.length) return
  if (fromIndex === toIndex) return
  const [moved] = tabs.splice(fromIndex, 1)
  tabs.splice(toIndex, 0, moved)
}

// ---- 会话进出与展开(FR-002/004) ----

export type OpenResult =
  | { kind: 'added'; tabId: string; collapsedSessionIds: string[] }
  | { kind: 'existing'; tabId: string; collapsedSessionIds: string[] }

/**
 * 「在工作台打开」:加入当前激活 tab 并自动展开;
 * 已在某 tab 则切到该 tab 并高亮其左列卡片,不重复添加(唯一性)。
 */
function openSession(sessionId: string): OpenResult {
  const found = findSession(sessionId)
  if (found) {
    state.value.activeTabId = found.tab.id
    flashSession(sessionId)
    return { kind: 'existing', tabId: found.tab.id, collapsedSessionIds: [] }
  }
  const tab = activeTab.value
  tab.sessionIds.push(sessionId)
  const expanded = expandSession(tab.id, sessionId)
  return { kind: 'added', tabId: tab.id, collapsedSessionIds: expanded.collapsedSessionIds }
}

/**
 * 应用内新建会话(FR-002 增强,替代经终端链路):前端生成 UUID 登记草稿,
 * 加入当前激活 tab 并展开。首条消息由 Rust 端以 --session-id 新建落盘,
 * 之后 watcher 刷新 projects,草稿被 pruneDrafts 收割,显示自动切换真实数据。
 */
function createDraftSession(cwd: string): string {
  const sessionId = crypto.randomUUID()
  state.value.drafts[sessionId] = cwd
  openSession(sessionId)
  return sessionId
}

/** 草稿会话的 cwd(非草稿返回 null)。各视图据此合成「新会话」占位 */
function draftCwd(sessionId: string): string | null {
  return state.value.drafts[sessionId] ?? null
}

/**
 * 草稿收割:已落盘(isPersisted)或已不在任何工作台(被关闭弃用)的草稿删除。
 * 由 App 层在 projects 刷新后调用。
 */
function pruneDrafts(isPersisted: (sessionId: string) => boolean) {
  for (const sid of Object.keys(state.value.drafts)) {
    if (isPersisted(sid) || !findSession(sid)) {
      delete state.value.drafts[sid]
    }
  }
}

export interface ExpandResult {
  /**
   * 软上限置换时被收回的会话(调用方弹瞬态 toast「已收起:<标题>」)。
   * 窗口缩小后已超员再展开时可能一次收回多个。
   */
  collapsedSessionIds: string[]
  /** 已展开时为聚焦而非新增 */
  focusedExisting: boolean
}

/**
 * 展开会话到右区(FR-004):列上限按容器宽度动态推算(每列 ≥ MIN_COLUMN_WIDTH),
 * 超出时最早展开(openedSeq 最小)的列自动收回。
 * atIndex 指定插入列位(拖拽落点);缺省追加末尾。
 */
function expandSession(tabId: string, sessionId: string, atIndex?: number): ExpandResult {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab || !tab.sessionIds.includes(sessionId)) {
    return { collapsedSessionIds: [], focusedExisting: false }
  }
  if (tab.columns.some(c => c.sessionId === sessionId)) {
    requestFocusColumn(sessionId)
    return { collapsedSessionIds: [], focusedExisting: true }
  }

  // 置换到容量内(窗口缩小后可能超员,循环收回最早展开者直到容得下新列)
  const collapsedSessionIds: string[] = []
  const max = dynamicMaxColumns()
  while (tab.columns.length >= max) {
    const earliest = tab.columns.reduce((min, c) => (c.openedSeq < min.openedSeq ? c : min))
    const removeIdx = tab.columns.findIndex(c => c.id === earliest.id)
    tab.columns.splice(removeIdx, 1)
    collapsedSessionIds.push(earliest.sessionId)
  }

  state.value.openSeq += 1
  const column: WorkbenchColumn = {
    id: genId('wbcol'),
    type: 'session',
    sessionId,
    openedSeq: state.value.openSeq,
  }
  const idx = atIndex === undefined ? tab.columns.length : Math.max(0, Math.min(atIndex, tab.columns.length))
  tab.columns.splice(idx, 0, column)
  tab.columnSizes = equalSizes(tab.columns.length)
  return { collapsedSessionIds, focusedExisting: false }
}

/** 收起列回左列(仍激活,FR-004「收起非退出」) */
function collapseColumn(tabId: string, sessionId: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const idx = tab.columns.findIndex(c => c.sessionId === sessionId)
  if (idx < 0) return
  tab.columns.splice(idx, 1)
  tab.columnSizes = equalSizes(tab.columns.length)
}

/** 退出工作台(左列 × / 列头 ×):从归属 tab 移除,展开列一并收回。调用方负责进行中确认 */
function removeSession(sessionId: string) {
  for (const tab of state.value.tabs) {
    const i = tab.sessionIds.indexOf(sessionId)
    if (i >= 0) {
      tab.sessionIds.splice(i, 1)
      const ci = tab.columns.findIndex(c => c.sessionId === sessionId)
      if (ci >= 0) {
        tab.columns.splice(ci, 1)
        tab.columnSizes = equalSizes(tab.columns.length)
      }
    }
  }
}

/**
 * 跨工作台移动(FR-005 拖拽②):移入目标 tab 左列末尾,不自动展开;
 * 源 tab 中已展开则先收起。
 */
function moveSessionToTab(sessionId: string, toTabId: string) {
  const found = findSession(sessionId)
  const target = state.value.tabs.find(t => t.id === toTabId)
  if (!target) return
  if (found?.tab.id === toTabId) return
  if (found) removeSession(sessionId)
  target.sessionIds.push(sessionId)
}

// ---- 右区列布局(FR-004/005) ----

function reorderColumns(tabId: string, fromIndex: number, toIndex: number) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const n = tab.columns.length
  if (fromIndex < 0 || fromIndex >= n || toIndex < 0 || toIndex >= n || fromIndex === toIndex) return
  const [col] = tab.columns.splice(fromIndex, 1)
  tab.columns.splice(toIndex, 0, col)
  const [size] = tab.columnSizes.splice(fromIndex, 1)
  tab.columnSizes.splice(toIndex, 0, size)
}

/**
 * 拖动第 index 条分隔线:
 * leftRatio 是 columns[index] 的目标新比例,仅在相邻两列间重分配,clamp 防压没。
 */
function updateColumnSize(tabId: string, index: number, leftRatio: number) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const sizes = tab.columnSizes
  if (index < 0 || index >= sizes.length - 1) return
  const combined = sizes[index] + sizes[index + 1]
  const minLeft = 0.1 * combined
  const maxLeft = 0.9 * combined
  const clamped = Math.max(minLeft, Math.min(maxLeft, leftRatio))
  sizes[index] = clamped
  sizes[index + 1] = combined - clamped
}

export function useWorkbench() {
  return {
    state,
    activeTab,
    flashSessionId,
    focusColumnRequest,
    findSession,
    isSessionVisibleInWorkbench,
    createTab,
    renameTab,
    closeTab,
    setActiveTab,
    reorderTabs,
    openSession,
    createDraftSession,
    draftCwd,
    pruneDrafts,
    expandSession,
    enforceColumnCapacity,
    collapseColumn,
    removeSession,
    moveSessionToTab,
    reorderColumns,
    updateColumnSize,
  }
}
