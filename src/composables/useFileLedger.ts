import { computed, type Ref, type ComputedRef } from 'vue'
import type { SessionRecord, ContentBlock } from '@/types'
import type { StreamingTurn } from './useStreaming'

/**
 * 会话文件账本(PRD v2.6.0 FR-001):从 records + streamingTurns 单遍扫描推导
 * 「本会话碰了哪些文件、怎么碰的」。纯前端只读推导,零 IPC、零落盘。
 *
 * 入账规则:Edit/Write/NotebookEdit 记修改,Read 记读取——仅识别 input 中单一
 * path 字段(file_path / notebook_path)为字符串的调用,确定性有保证;
 * Glob/Grep/Bash 及其余工具一律不入账(Bash 改文件是声明过的盲区)。
 * modified 粘性:出现过任一修改操作,文件即为 modified。
 *
 * 性能契约:computed 依赖收集只触及块的 type/name/input/id——流式 text/thinking
 * 的字符增长不触发重算,仅新块挂载与工具参数落定(block_stop 赋 input)触发,
 * 天然不进每帧热路径(NFR-001)。
 */

export interface LedgerOp {
  tool: 'Edit' | 'Write' | 'NotebookEdit' | 'Read'
  /** 跳转锚点:tool_use 块自带 id——工具卡 DOM 有 data-tool-use-id,locateToolUse 直接定位 */
  anchorId: string
  /** ISO 时间;流式 turn 的操作以入账时刻近似(JSONL 未落盘无真值) */
  timestamp: string | null
  /** Edit 专有 */
  oldString?: string
  newString?: string
  replaceAll?: boolean
  /** Write 专有:content 存引用(与 records 共享,不复制),摘要字段单列 */
  content?: string
  contentChars?: number
  contentLines?: number
}

export interface FileEntry {
  /** 归一化后的绝对路径(字符串级归一,不做 fs 调用) */
  path: string
  modified: boolean
  /** 账本推断的"新建":该文件首个操作是 Write(非文件系统真值,UI 徽标用) */
  createdByWrite: boolean
  ops: LedgerOp[]
  readCount: number
  editCount: number
  firstTs: string | null
  lastTs: string | null
}

const MODIFY_TOOLS = new Set(['Edit', 'Write', 'NotebookEdit'])

/** 字符串级路径归一:折叠重复分隔符、去 ./ 段、去尾随 /。不 canonicalize(保持纯函数) */
export function normalizeLedgerPath(p: string): string {
  let s = p.replace(/\/{2,}/g, '/')
  while (s.includes('/./')) s = s.replace('/./', '/')
  if (s.startsWith('./')) s = s.slice(2)
  if (s.length > 1 && s.endsWith('/')) s = s.slice(0, -1)
  return s
}

function ingestBlocks(
  map: Map<string, FileEntry>,
  seenToolUse: Set<string>,
  blocks: ContentBlock[],
  anchorId: string,
  timestamp: string | null,
): void {
  for (const b of blocks) {
    if (!b || b.type !== 'tool_use') continue
    const name = (b as { name?: string }).name ?? ''
    const isModify = MODIFY_TOOLS.has(name)
    if (!isModify && name !== 'Read') continue
    const input = (b as { input?: unknown }).input
    if (!input || typeof input !== 'object') continue
    const inp = input as Record<string, unknown>
    const rawPath = name === 'NotebookEdit' ? inp.notebook_path : inp.file_path
    if (typeof rawPath !== 'string' || !rawPath) continue
    // 去重:tool_use 自带全局唯一 id——settle 时序下同一调用可能短暂双在
    // records 与 streamingTurns(账本层的双显防护)
    const toolUseId = (b as { id?: string }).id
    if (toolUseId) {
      if (seenToolUse.has(toolUseId)) continue
      seenToolUse.add(toolUseId)
    }
    const path = normalizeLedgerPath(rawPath)
    let entry = map.get(path)
    if (!entry) {
      entry = {
        path, modified: false, createdByWrite: false,
        ops: [], readCount: 0, editCount: 0, firstTs: null, lastTs: null,
      }
      map.set(path, entry)
    }
    const op: LedgerOp = { tool: name as LedgerOp['tool'], anchorId: toolUseId ?? anchorId, timestamp }
    if (name === 'Edit') {
      op.oldString = typeof inp.old_string === 'string' ? inp.old_string : ''
      op.newString = typeof inp.new_string === 'string' ? inp.new_string : ''
      op.replaceAll = inp.replace_all === true
    } else if (name === 'Write') {
      const c = typeof inp.content === 'string' ? inp.content : ''
      op.content = c
      op.contentChars = c.length
      op.contentLines = c ? c.split('\n').length : 0
    }
    if (isModify) {
      if (entry.ops.length === 0 && name === 'Write') entry.createdByWrite = true
      entry.modified = true
      entry.editCount++
    } else {
      entry.readCount++
    }
    entry.ops.push(op)
    if (!entry.firstTs) entry.firstTs = timestamp
    if (timestamp) entry.lastTs = timestamp
  }
}

/** 纯函数推导(可单测):返回按最后触碰倒序的文件条目数组 */
export function buildFileLedger(
  records: readonly SessionRecord[],
  turns: readonly StreamingTurn[],
): FileEntry[] {
  const map = new Map<string, FileEntry>()
  const seen = new Set<string>()
  for (const r of records) {
    if (r.type !== 'assistant') continue
    if (r.is_sidechain) continue
    const content = r.message?.content
    if (!Array.isArray(content)) continue
    ingestBlocks(map, seen, content, r.uuid ?? '', r.timestamp)
  }
  // 流式 turn:JSONL 未落盘无真实时间戳,以入账时刻近似(保证实时条目排最前)
  const nowIso = turns.length ? new Date().toISOString() : ''
  for (const t of turns) {
    ingestBlocks(map, seen, t.content, t.messageId, nowIso || null)
  }
  return [...map.values()].sort((a, b) => (b.lastTs ?? '').localeCompare(a.lastTs ?? ''))
}

/** 响应式包装:records/turns 变化时重算(依赖粒度见文件头注释) */
export function useFileLedger(
  records: Ref<readonly SessionRecord[] | null>,
  turns: ComputedRef<readonly StreamingTurn[]> | Ref<readonly StreamingTurn[]>,
) {
  const entries = computed(() => buildFileLedger(records.value ?? [], turns.value))
  const modifiedEntries = computed(() => entries.value.filter(e => e.modified))
  const readOnlyEntries = computed(() => entries.value.filter(e => !e.modified))
  return { entries, modifiedEntries, readOnlyEntries }
}
