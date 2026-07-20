<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { FileEntry } from '@/composables/useFileLedger'
import FileLedgerTimeline from './FileLedgerTimeline.vue'

/** 文件账本面板(PRD v2.6.0 FR-002/FR-004):清单态 + 面板内下钻 + git 只读快照条 */
const props = defineProps<{
  modified: FileEntry[]
  readOnly: FileEntry[]
  cwd: string | null
}>()

const emit = defineEmits<{
  close: []
  locate: [toolUseId: string]
}>()

const readOpen = ref(false)
const selected = ref<FileEntry | null>(null)

/** 会话内工具卡直达:按 tool_use 锚点下钻到所属文件时间线(镜像 AsyncTaskPanel.openByToolUse) */
function openByAnchor(anchorId: string): boolean {
  const entry = [...props.modified, ...props.readOnly]
    .find(e => e.ops.some(op => op.anchorId === anchorId))
  if (!entry) return false
  selected.value = entry
  return true
}
defineExpose({ openByAnchor })

// 下钻条目跟随账本实时更新(流式新操作到达时按 path 重取)
const selectedLive = computed<FileEntry | null>(() => {
  if (!selected.value) return null
  const p = selected.value.path
  return props.modified.find(e => e.path === p) ?? props.readOnly.find(e => e.path === p) ?? selected.value
})

/** cwd 相对短显,越界(不在 cwd 下)保持绝对路径 */
function shortPath(p: string): string {
  const c = props.cwd
  if (c && p.startsWith(c + '/')) return p.slice(c.length + 1)
  return p
}

function relTime(iso: string | null): string {
  if (!iso) return ''
  const dt = Date.now() - new Date(iso).getTime()
  const m = Math.floor(dt / 60000)
  if (m < 1) return 'now'
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h`
  return `${Math.floor(h / 24)}d`
}

// ---- git 只读快照(FR-004):打开时一次 + 手动刷新;失败/非仓库静默隐藏 ----
interface GitSnapshot {
  is_repo: boolean
  entries: { status: string; path: string }[]
  truncated: boolean
}
const snapshot = ref<GitSnapshot | null>(null)
const snapLoading = ref(false)
const snapOpen = ref(false)

async function refreshSnapshot() {
  if (!props.cwd || snapLoading.value) return
  snapLoading.value = true
  try {
    snapshot.value = await invoke<GitSnapshot>('git_worktree_snapshot', { cwd: props.cwd })
  } catch {
    snapshot.value = null
  } finally {
    snapLoading.value = false
  }
}
watch(() => props.cwd, refreshSnapshot, { immediate: true })

const snapCounts = computed(() => {
  const s = snapshot.value
  if (!s?.is_repo) return null
  let modified = 0
  let untracked = 0
  let staged = 0
  for (const e of s.entries) {
    if (e.status === '??') untracked++
    else if (e.status[0] !== ' ' && e.status[0] !== '?') staged++
    else modified++
  }
  return { modified, untracked, staged, total: s.entries.length }
})
</script>

<template>
  <div class="h-full flex flex-col text-sm">
    <!-- 下钻态 -->
    <FileLedgerTimeline
      v-if="selectedLive"
      :entry="selectedLive"
      :short-path="shortPath(selectedLive.path)"
      @back="selected = null"
      @locate="emit('locate', $event)"
    />

    <!-- 清单态 -->
    <template v-else>
      <div class="flex items-center gap-1.5 px-3 py-2 border-b border-border shrink-0">
        <span class="i-carbon-catalog w-3.5 h-3.5 text-claude" />
        <span class="font-medium text-xs">{{ $t('fileLedger.title') }}</span>
        <span class="text-[10px] text-muted-foreground">
          {{ $t('fileLedger.summary', { total: modified.length + readOnly.length, modified: modified.length, read: readOnly.length }) }}
        </span>
        <button class="ml-auto p-0.5 rounded hover:bg-muted text-muted-foreground" @click="emit('close')">
          <span class="i-carbon-close w-3.5 h-3.5" />
        </button>
      </div>

      <div class="flex-1 overflow-y-auto px-3 py-2 space-y-2.5">
        <p v-if="!modified.length && !readOnly.length" class="text-xs text-muted-foreground text-center py-8">
          {{ $t('fileLedger.empty') }}
        </p>

        <!-- 已修改 -->
        <div v-if="modified.length" class="rounded-md border border-border bg-card px-2.5 py-2">
          <div class="flex items-center gap-1.5 text-[10px] font-semibold text-muted-foreground mb-1">
            <span class="w-1.5 h-1.5 rounded-full bg-destructive/70" />
            {{ $t('fileLedger.modifiedGroup', { n: modified.length }) }}
          </div>
          <button
            v-for="e in modified" :key="e.path"
            class="w-full flex items-center gap-2 px-1 py-1 rounded hover:bg-muted/60 text-left"
            :title="e.path"
            @click="selected = e"
          >
            <span class="flex-1 font-mono text-[11px] truncate" dir="rtl"><bdi>{{ shortPath(e.path) }}</bdi></span>
            <span
              class="text-[9px] px-1.5 rounded-full whitespace-nowrap"
              :class="e.createdByWrite ? 'bg-claude/15 text-claude' : 'bg-destructive/10 text-destructive'"
            >{{ e.createdByWrite ? $t('fileLedger.newFile') : $t('fileLedger.edits', { n: e.editCount }) }}</span>
            <span class="text-[9px] text-muted-foreground tabular-nums w-7 text-right">{{ relTime(e.lastTs) }}</span>
          </button>
        </div>

        <!-- 仅读取(默认折叠) -->
        <div v-if="readOnly.length" class="rounded-md border border-border bg-card px-2.5 py-2">
          <button class="w-full flex items-center gap-1.5 text-[10px] font-semibold text-muted-foreground" @click="readOpen = !readOpen">
            <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/50" />
            {{ $t('fileLedger.readGroup', { n: readOnly.length }) }}
            <span class="ml-auto i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': readOpen }" />
          </button>
          <template v-if="readOpen">
            <div v-for="e in readOnly" :key="e.path" class="flex items-center gap-2 px-1 py-1" :title="e.path">
              <span class="flex-1 font-mono text-[11px] truncate text-muted-foreground" dir="rtl"><bdi>{{ shortPath(e.path) }}</bdi></span>
              <span class="text-[9px] text-muted-foreground tabular-nums w-7 text-right">{{ relTime(e.lastTs) }}</span>
            </div>
          </template>
        </div>
      </div>

      <!-- 工作区快照条 + 脚注 -->
      <div class="shrink-0 border-t border-dashed border-border px-3 py-2">
        <div v-if="snapCounts" class="flex items-center gap-1.5 text-[10px]">
          <span class="font-semibold text-muted-foreground">{{ $t('fileLedger.worktree') }}</span>
          <button class="flex items-center gap-1.5" @click="snapOpen = !snapOpen">
            <span v-if="snapCounts.modified" class="font-mono px-1.5 rounded border border-border bg-card"><b>M</b> {{ snapCounts.modified }}</span>
            <span v-if="snapCounts.staged" class="font-mono px-1.5 rounded border border-border bg-card"><b>S</b> {{ snapCounts.staged }}</span>
            <span v-if="snapCounts.untracked" class="font-mono px-1.5 rounded border border-border bg-card"><b>??</b> {{ snapCounts.untracked }}</span>
            <span v-if="!snapCounts.total" class="text-muted-foreground">{{ $t('fileLedger.clean') }}</span>
          </button>
          <button class="ml-auto p-0.5 rounded hover:bg-muted text-muted-foreground" :disabled="snapLoading" @click="refreshSnapshot">
            <span class="i-carbon-renew w-3 h-3" :class="{ 'animate-spin': snapLoading }" />
          </button>
        </div>
        <div v-if="snapOpen && snapshot?.entries.length" class="mt-1.5 max-h-32 overflow-y-auto space-y-0.5">
          <div v-for="e in snapshot.entries" :key="e.path" class="flex gap-2 text-[10px] font-mono">
            <span class="text-muted-foreground w-5">{{ e.status }}</span>
            <span class="truncate">{{ e.path }}</span>
          </div>
          <div v-if="snapshot.truncated" class="text-[10px] text-muted-foreground">{{ $t('fileLedger.andMore') }}</div>
        </div>
        <p class="text-[9px] text-muted-foreground/70 mt-1.5">{{ $t('fileLedger.footnote') }}</p>
      </div>
    </template>
  </div>
</template>
