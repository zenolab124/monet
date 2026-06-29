import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import i18n from '../locales'

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

export interface RaceLane {
  id: string
  sessionId: string
  label: string
}

export interface RaceConfig {
  cwd: string
  lanes: RaceLane[]
}

export interface WorkbenchTab {
  id: string
  name: string
  /** 左列会话,加入顺序即显示顺序(任何状态变化不重排) */
  sessionIds: string[]
  /** 右区展开列(数组序 = 列序,可拖拽重排) */
  columns: WorkbenchColumn[]
  /** 各列像素宽度,与 columns 平行 */
  columnSizes: number[]
  /** 赛马模式配置。非 undefined 即赛马 Tab */
  race?: RaceConfig
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
const MIN_WIDTH_KEY = 'cc-space-min-column-width'
const DEFAULT_MIN_COLUMN_WIDTH = 360
const ABSOLUTE_MIN_COLUMN_WIDTH = 200

const minColumnWidth = ref(
  Math.max(ABSOLUTE_MIN_COLUMN_WIDTH, Number(localStorage.getItem(MIN_WIDTH_KEY)) || DEFAULT_MIN_COLUMN_WIDTH)
)

/** 右区四周边距与列间隙(与 WorkbenchColumns 的 PAD/GAP 一致) */
const COLUMN_GAP = 10

/**
 * 右区容器实测宽度:WorkbenchColumns 挂载后经 ResizeObserver 维护。
 * v-show 隐藏报 0 不更新(保留最后有效值);初始按窗口减 ActivityBar(48)+左列(256) 估算。
 */
const rightZoneWidth = ref(Math.max(minColumnWidth.value, window.innerWidth - 48 - 256))

export function setRightZoneWidth(w: number) {
  if (w <= 0) return
  const prev = rightZoneWidth.value
  rightZoneWidth.value = w
  if (w > prev) redistributeOnGrow()
}

function containerFreeWidth(n: number): number {
  return rightZoneWidth.value - COLUMN_GAP * Math.max(0, n - 1) - COLUMN_GAP * 2
}

/** 窗口变大时,按比例放大各列填满(仅当前全部列已 fit 时触发) */
function redistributeOnGrow() {
  for (const tab of state.value.tabs) {
    if (tab.race || tab.columns.length === 0) continue
    const free = containerFreeWidth(tab.columns.length)
    const total = tab.columnSizes.reduce((s, w) => s + w, 0)
    if (total <= 0 || total > free) continue
    const scale = free / total
    tab.columnSizes = tab.columnSizes.map(w => Math.max(minColumnWidth.value, Math.round(w * scale)))
  }
}

let idCounter = 0
function genId(prefix: string) {
  return `${prefix}-${++idCounter}-${Date.now().toString(36)}`
}

function equalSizes(n: number): number[] {
  if (n <= 0) return []
  const free = containerFreeWidth(n)
  const w = Math.max(minColumnWidth.value, Math.round(free / n))
  return Array(n).fill(w)
}

function createTabObject(seq: number): WorkbenchTab {
  return {
    id: genId('wbtab'),
    name: i18n.global.t('workbench.defaultTabName', { seq }),
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
      if (t.columnSizes.some(s => typeof s !== 'number' || !Number.isFinite(s) || s < 0)) return null
      if (t.race !== undefined) {
        if (!t.race || typeof t.race !== 'object') return null
        if (typeof t.race.cwd !== 'string' || !t.race.cwd) return null
        if (!Array.isArray(t.race.lanes) || t.race.lanes.length === 0) return null
        for (const lane of t.race.lanes) {
          if (!lane || typeof lane.id !== 'string' || typeof lane.sessionId !== 'string') return null
          if (typeof lane.label !== 'string') return null
          if (!t.sessionIds.includes(lane.sessionId)) return null
          if (!t.columns.some(c => c.sessionId === lane.sessionId)) return null
        }
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

// ratio → pixel 迁移:旧版 columnSizes 为比例(均 < minColumnWidth.value),转为像素宽度
if (loaded) {
  for (const tab of loaded.tabs) {
    if (tab.columns.length > 0 && tab.columnSizes.length > 0 && Math.max(...tab.columnSizes) < minColumnWidth.value) {
      const free = Math.max(minColumnWidth.value, window.innerWidth - 48 - 256) - COLUMN_GAP * Math.max(0, tab.columns.length - 1) - COLUMN_GAP * 2
      tab.columnSizes = tab.columnSizes.map(r => Math.max(minColumnWidth.value, Math.round(r * free)))
    }
  }
}

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
  const removed = state.value.tabs.splice(idx, 1)[0]
  if (state.value.activeTabId === tabId) {
    state.value.activeTabId = state.value.tabs[Math.max(0, idx - 1)].id
  }
  for (const sid of removed.sessionIds) teardownSession(sid)
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

function reorderSessions(tabId: string, fromIndex: number, toIndex: number) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const n = tab.sessionIds.length
  if (fromIndex < 0 || fromIndex >= n || toIndex < 0 || toIndex >= n || fromIndex === toIndex) return
  const [moved] = tab.sessionIds.splice(fromIndex, 1)
  tab.sessionIds.splice(toIndex, 0, moved)
}

// ---- 赛马模式 ----

/**
 * 从已有会话发起赛马:原会话迁入新赛马 Tab 为 lane 1,
 * 再分叉一份为 lane 2。调用方负责先调 Rust fork_session 完成文件复制。
 */
function createRaceTab(sourceSessionId: string, cwd: string, forkedSessionId: string): WorkbenchTab {
  removeSession(sourceSessionId)

  state.value.tabSeq += 1
  const tab = createTabObject(state.value.tabSeq)
  tab.name = i18n.global.t('workbench.race.defaultTabName', { seq: state.value.tabSeq })

  const lanes: RaceLane[] = []
  for (const sid of [sourceSessionId, forkedSessionId]) {
    tab.sessionIds.push(sid)
    state.value.openSeq += 1
    tab.columns.push({
      id: genId('wbcol'),
      type: 'session',
      sessionId: sid,
      openedSeq: state.value.openSeq,
    })
    lanes.push({
      id: genId('lane'),
      sessionId: sid,
      label: i18n.global.t('workbench.race.laneLabel', { n: lanes.length + 1 }),
    })
  }

  tab.columnSizes = equalSizes(lanes.length)
  tab.race = { cwd, lanes }

  state.value.tabs.push(tab)
  state.value.activeTabId = tab.id
  return tab
}

/** 向赛马 Tab 追加一个分叉赛道。调用方负责先完成文件复制 */
function addRaceLane(tabId: string, forkedSessionId: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab?.race) return

  tab.sessionIds.push(forkedSessionId)
  state.value.openSeq += 1
  tab.columns.push({
    id: genId('wbcol'),
    type: 'session',
    sessionId: forkedSessionId,
    openedSeq: state.value.openSeq,
  })
  tab.race.lanes.push({
    id: genId('lane'),
    sessionId: forkedSessionId,
    label: i18n.global.t('workbench.race.laneLabel', { n: tab.race.lanes.length + 1 }),
  })
  tab.columnSizes = equalSizes(tab.columns.length)
}

/** 关闭赛马赛道:移除列 + lane;剩 1 条时自动解散为普通 Tab */
function removeRaceLane(tabId: string, sessionId: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab?.race) return

  tab.race.lanes = tab.race.lanes.filter(l => l.sessionId !== sessionId)
  const si = tab.sessionIds.indexOf(sessionId)
  if (si >= 0) tab.sessionIds.splice(si, 1)
  const ci = tab.columns.findIndex(c => c.sessionId === sessionId)
  if (ci >= 0) tab.columns.splice(ci, 1)
  tab.columnSizes = equalSizes(tab.columns.length)

  if (tab.race.lanes.length <= 1) {
    delete tab.race
  }
}

/** 重置所有赛道：保留赛道数、cwd 和每条赛道的设置（模型/强度/渠道），只清空会话 */
function resetRaceLanes(tabId: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab?.race) return
  const cwd = tab.race.cwd
  const oldLanes = tab.race.lanes

  const oldSettings: Array<{ sid: string; raw: string | null }> = oldLanes.map(lane => ({
    sid: lane.sessionId,
    raw: localStorage.getItem(`cc-space:session-settings:${lane.sessionId}`),
  }))

  for (const lane of oldLanes) {
    teardownSession(lane.sessionId)
  }

  tab.sessionIds = []
  tab.columns = []
  const lanes: RaceLane[] = []
  for (let i = 0; i < oldLanes.length; i++) {
    const sid = crypto.randomUUID()
    state.value.drafts[sid] = cwd
    tab.sessionIds.push(sid)
    state.value.openSeq += 1
    tab.columns.push({
      id: genId('wbcol'),
      type: 'session',
      sessionId: sid,
      openedSeq: state.value.openSeq,
    })
    lanes.push({
      id: genId('lane'),
      sessionId: sid,
      label: i18n.global.t('workbench.race.laneLabel', { n: i + 1 }),
    })
    if (oldSettings[i].raw) {
      localStorage.setItem(`cc-space:session-settings:${sid}`, oldSettings[i].raw!)
    }
  }
  tab.columnSizes = equalSizes(lanes.length)
  tab.race = { cwd, lanes }
}

function findLane(tab: WorkbenchTab, sessionId: string): RaceLane | null {
  return tab.race?.lanes.find(l => l.sessionId === sessionId) ?? null
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
  collapsedSessionIds: string[]
  focusedExisting: boolean
}

/**
 * 展开会话到右区:无容量上限,超出容器时横向滚动。
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
  requestFocusColumn(sessionId)
  return { collapsedSessionIds: [], focusedExisting: false }
}

/**
 * 移除列并智能回收宽度:
 * - 溢出态(有滚动):直接丢弃宽度,总宽减少;若丢弃后能 fit 则按比例填满
 * - 非溢出态:邻居吃掉宽度,保持填满容器
 */
function reclaimColumnWidth(tab: WorkbenchTab, removedIndex: number) {
  const freed = tab.columnSizes[removedIndex]
  const totalBefore = tab.columnSizes.reduce((s, w) => s + w, 0)
  const freeBefore = containerFreeWidth(tab.columns.length)
  const wasOverflowing = totalBefore > freeBefore

  tab.columns.splice(removedIndex, 1)
  tab.columnSizes.splice(removedIndex, 1)

  if (tab.columnSizes.length === 0) return

  if (wasOverflowing) {
    const newTotal = totalBefore - freed
    const newFree = containerFreeWidth(tab.columnSizes.length)
    if (newTotal < newFree) {
      const scale = newFree / newTotal
      tab.columnSizes = tab.columnSizes.map(w => Math.max(minColumnWidth.value, Math.round(w * scale)))
    }
  } else {
    const neighbor = Math.min(removedIndex, tab.columnSizes.length - 1)
    tab.columnSizes[neighbor] += freed
  }
}

/** 收起列回左列(仍激活,「收起非退出」) */
function collapseColumn(tabId: string, sessionId: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const idx = tab.columns.findIndex(c => c.sessionId === sessionId)
  if (idx < 0) return
  reclaimColumnWidth(tab, idx)
}

/** 退出工作台(左列 × / 列头 ×):从归属 tab 移除,展开列一并收回 */
function removeSession(sessionId: string) {
  for (const tab of state.value.tabs) {
    const i = tab.sessionIds.indexOf(sessionId)
    if (i >= 0) {
      tab.sessionIds.splice(i, 1)
      const ci = tab.columns.findIndex(c => c.sessionId === sessionId)
      if (ci >= 0) reclaimColumnWidth(tab, ci)
    }
  }
  teardownSession(sessionId)
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
 * 拖动第 index 条分隔线(像素宽度模型):
 * - 最后一列:独立调整宽度(无右邻,拉宽触发滚动)
 * - 中间列:有余量时此消彼长;右列顶到 minColumnWidth.value 后独立拉宽
 */
function updateColumnSize(tabId: string, index: number, desiredLeftWidth: number) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab) return
  const sizes = tab.columnSizes
  if (index < 0 || index >= sizes.length) return
  const left = Math.max(minColumnWidth.value, Math.round(desiredLeftWidth))
  if (index === sizes.length - 1) {
    sizes[index] = left
    return
  }
  const combined = sizes[index] + sizes[index + 1]
  const rightFromZeroSum = combined - left
  if (rightFromZeroSum >= minColumnWidth.value) {
    sizes[index] = left
    sizes[index + 1] = rightFromZeroSum
  } else {
    sizes[index] = left
    sizes[index + 1] = minColumnWidth.value
  }
}

/** 会话离开工作台后,若不再被任何 tab 持有则关闭进程(断 Remote Control) */
function teardownSession(sessionId: string) {
  const stillReferenced = state.value.tabs.some(t => t.sessionIds.includes(sessionId))
  if (!stillReferenced) {
    invoke('close_session', { sessionId }).catch(() => {})
  }
}

function resetColumnSizes(tabId: string) {
  const tab = state.value.tabs.find(t => t.id === tabId)
  if (!tab || tab.columns.length === 0) return
  tab.columnSizes = equalSizes(tab.columns.length)
}

function setMinColumnWidth(w: number) {
  const clamped = Math.max(ABSOLUTE_MIN_COLUMN_WIDTH, Math.round(w))
  minColumnWidth.value = clamped
  localStorage.setItem(MIN_WIDTH_KEY, String(clamped))
}

export function useWorkbench() {
  return {
    state,
    activeTab,
    minColumnWidth,
    flashSessionId,
    focusColumnRequest,
    findSession,
    isSessionVisibleInWorkbench,
    createTab,
    createRaceTab,
    addRaceLane,
    removeRaceLane,
    resetRaceLanes,
    findLane,
    renameTab,
    closeTab,
    setActiveTab,
    reorderTabs,
    reorderSessions,
    openSession,
    createDraftSession,
    draftCwd,
    pruneDrafts,
    expandSession,
    collapseColumn,
    removeSession,
    moveSessionToTab,
    reorderColumns,
    updateColumnSize,
    resetColumnSizes,
    setMinColumnWidth,
  }
}
