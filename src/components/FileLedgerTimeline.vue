<script setup lang="ts">
import { ref, computed } from 'vue'
import { diffLines, type Change } from 'diff'
import type { FileEntry, LedgerOp } from '@/composables/useFileLedger'

/** 单文件操作时间线(PRD v2.6.0 FR-003):Edit 懒 diff、Write 摘要、跳转工具卡 */
const props = defineProps<{
  entry: FileEntry
  shortPath: string
}>()

const emit = defineEmits<{
  back: []
  locate: [toolUseId: string]
}>()

const DIFF_FOLD_LINES = 50
const WRITE_PREVIEW_LINES = 200

interface DiffRow { kind: 'add' | 'del' | 'ctx'; text: string }

// diff 懒计算 + 按锚点缓存(面板关闭即弃);已渲染项不因实时追加重算
const diffCache = new Map<string, DiffRow[]>()
const expandedDiffs = ref(new Set<string>())
const expandedPreviews = ref(new Set<string>())

function diffOf(op: LedgerOp): DiffRow[] {
  const key = op.anchorId
  const hit = diffCache.get(key)
  if (hit) return hit
  const rows: DiffRow[] = []
  try {
    const changes: Change[] = diffLines(op.oldString ?? '', op.newString ?? '')
    for (const c of changes) {
      const kind: DiffRow['kind'] = c.added ? 'add' : c.removed ? 'del' : 'ctx'
      for (const line of c.value.replace(/\n$/, '').split('\n')) rows.push({ kind, text: line })
    }
  } catch {
    rows.push({ kind: 'ctx', text: '(diff unavailable)' })
  }
  diffCache.set(key, rows)
  return rows
}

function visibleRows(op: LedgerOp): DiffRow[] {
  const rows = diffOf(op)
  if (rows.length <= DIFF_FOLD_LINES || expandedDiffs.value.has(op.anchorId)) return rows
  return rows.slice(0, DIFF_FOLD_LINES)
}

function foldedCount(op: LedgerOp): number {
  const rows = diffOf(op)
  return rows.length > DIFF_FOLD_LINES && !expandedDiffs.value.has(op.anchorId) ? rows.length : 0
}

function previewLines(op: LedgerOp): string[] {
  return (op.content ?? '').split('\n').slice(0, WRITE_PREVIEW_LINES)
}

function timeOf(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

const ops = computed(() => props.entry.ops)
</script>

<template>
  <div class="h-full flex flex-col">
    <div class="px-3 py-2 border-b border-border shrink-0">
      <button class="text-[11px] text-claude flex items-center gap-1" @click="emit('back')">
        <span class="i-carbon-chevron-left w-3 h-3" />{{ $t('fileLedger.title') }}
      </button>
      <div class="font-mono text-[11.5px] font-semibold mt-1 break-all">{{ shortPath }}</div>
    </div>

    <div class="flex-1 overflow-y-auto px-3 py-3">
      <div
        v-for="op in ops" :key="op.anchorId"
        class="relative pl-3.5 pb-4 ml-1 border-l-2 border-border last:pb-1"
      >
        <span
          class="absolute -left-[5px] top-0.5 w-2 h-2 rounded-full border-2 bg-background"
          :class="op.tool === 'Write' ? 'border-destructive/70' : op.tool === 'Read' ? 'border-muted-foreground/50' : 'border-claude'"
        />
        <div class="flex items-center gap-2 text-[11px] mb-1">
          <span class="font-semibold">{{ op.tool }}</span>
          <span v-if="op.replaceAll" class="text-[8px] px-1 rounded bg-destructive/10 text-destructive">replace_all</span>
          <span class="text-[10px] text-muted-foreground tabular-nums">{{ timeOf(op.timestamp) }}</span>
          <button class="ml-auto text-[10px] text-claude hover:underline" @click="emit('locate', op.anchorId)">
            {{ $t('fileLedger.jump') }} ↗
          </button>
        </div>

        <!-- Edit: 行级 diff -->
        <div v-if="op.tool === 'Edit'" class="rounded border border-border bg-card overflow-hidden font-mono text-[10.5px] leading-[1.5]">
          <div
            v-for="(row, i) in visibleRows(op)" :key="i"
            class="px-2 whitespace-pre overflow-hidden text-ellipsis"
            :class="row.kind === 'add' ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-400'
              : row.kind === 'del' ? 'bg-destructive/10 text-destructive'
              : 'text-muted-foreground'"
          >{{ (row.kind === 'add' ? '+ ' : row.kind === 'del' ? '- ' : '  ') + row.text }}</div>
          <button
            v-if="foldedCount(op)"
            class="w-full text-center text-[10px] text-claude py-0.5 border-t border-dashed border-border hover:bg-muted/50"
            @click="expandedDiffs.add(op.anchorId)"
          >{{ $t('fileLedger.expandAll', { n: foldedCount(op) }) }} ▾</button>
        </div>

        <!-- Write: 摘要 + 可展开预览 -->
        <div v-else-if="op.tool === 'Write'" class="rounded border border-border bg-card px-2 py-1.5 text-[10.5px]">
          <span class="text-muted-foreground">{{ $t('fileLedger.writeSummary', { lines: op.contentLines ?? 0, chars: op.contentChars ?? 0 }) }}</span>
          <button
            class="text-claude ml-1.5 hover:underline"
            @click="expandedPreviews.has(op.anchorId) ? expandedPreviews.delete(op.anchorId) : expandedPreviews.add(op.anchorId)"
          >{{ expandedPreviews.has(op.anchorId) ? $t('fileLedger.collapse') : $t('fileLedger.preview') }}</button>
          <pre
            v-if="expandedPreviews.has(op.anchorId)"
            class="mt-1.5 font-mono text-[10px] leading-[1.5] max-h-64 overflow-auto whitespace-pre bg-muted/30 rounded p-1.5"
          >{{ previewLines(op).join('\n') }}</pre>
          <div v-if="expandedPreviews.has(op.anchorId) && (op.contentLines ?? 0) > WRITE_PREVIEW_LINES" class="text-[9px] text-muted-foreground mt-0.5">
            {{ $t('fileLedger.previewTruncated', { total: op.contentLines }) }}
          </div>
        </div>

        <!-- Read / NotebookEdit: 单行摘要 -->
        <div v-else class="text-[10.5px] text-muted-foreground">
          {{ op.tool === 'Read' ? $t('fileLedger.readOp') : $t('fileLedger.notebookOp') }}
        </div>
      </div>
    </div>
  </div>
</template>
