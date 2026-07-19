<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useMemory, MEMORY_STALE_DAYS } from '@/composables/useMemory'
import { relativeTime } from '@/types'
import { renderMarkdownCached } from '@/composables/useMarkdown'

/**
 * 记忆详情面板（v2.9.0 FR-007/FR-009）：
 * frontmatter 卡 + 正文 markdown（含 wiki-link 处理）+ 编辑/删除动作。
 */

const { t } = useI18n()
const {
  selectedFile,
  selectedEntry,
  detail,
  detailLoading,
  detailError,
  loadDetail,
  isWikiLinkValid,
  currentProject,
} = useMemory()

const emit = defineEmits<{
  (e: 'edit'): void
  (e: 'delete'): void
}>()

// 陈旧判断
const isStale = computed(() => {
  if (!selectedEntry.value) return false
  if (selectedEntry.value.type !== 'project') return false
  const now = Date.now() / 1000
  const days = (now - selectedEntry.value.mtime) / 86400
  return days > MEMORY_STALE_DAYS
})

// frontmatter 条目
const frontmatterEntries = computed(() => {
  const fm = detail.value?.frontmatter
  if (!fm) return []
  return Object.entries(fm).map(([key, value]) => ({
    key,
    value: typeof value === 'object' ? JSON.stringify(value) : String(value ?? ''),
  }))
})

// mtime 格式化
const mtimeDisplay = computed(() => {
  if (!detail.value) return ''
  const date = new Date(detail.value.mtime * 1000)
  return `${date.toLocaleDateString('zh-CN')} (${relativeTime(detail.value.mtime)})`
})

/**
 * 正文 markdown 渲染：先预处理 [[slug]] wiki-link，
 * 存在的目标 → 可点样式；断链 → 红虚线样式。
 */
const renderedBody = computed(() => {
  if (!detail.value?.body) return ''
  // 预处理 wiki-links：替换 [[slug]] 为特殊 HTML
  const processed = detail.value.body.replace(/\[\[([^\]]+)\]\]/g, (_match, slug: string) => {
    const valid = isWikiLinkValid(slug)
    if (valid) {
      return `<span class="wikilink" data-wiki-slug="${slug}">[[${slug}]]</span>`
    } else {
      return `<span class="wikilink-broken" title="${t('memory.brokenLinkTitle')}">[[${slug}]]</span>`
    }
  })
  return renderMarkdownCached(processed)
})

// wiki-link 点击处理（事件委托）
function handleBodyClick(event: MouseEvent) {
  const target = event.target as HTMLElement
  const wikiEl = target.closest('.wikilink') as HTMLElement | null
  if (!wikiEl) return
  const slug = wikiEl.dataset.wikiSlug
  if (!slug) return
  // 切换选中到目标文件
  const targetFile = slug + '.md'
  const exists = currentProject.value?.entries.some(e => e.file === targetFile)
  if (exists) {
    selectedFile.value = targetFile
  }
}

function retry() {
  if (selectedFile.value) loadDetail(selectedFile.value)
}
</script>

<template>
  <!-- 空态：未选中 -->
  <div v-if="!selectedFile" class="detail-empty">
    <span class="text-xs text-muted-foreground">{{ t('memory.selectToView') }}</span>
  </div>

  <!-- 加载态 -->
  <div v-else-if="detailLoading" class="detail-empty">
    <span class="text-xs text-muted-foreground">{{ t('common.loading') }}</span>
  </div>

  <!-- 错误态 -->
  <div v-else-if="detailError" class="detail-error">
    <p class="text-xs text-destructive">{{ t('common.loadFailed') }}</p>
    <button class="ws-btn" @click="retry">{{ t('common.retry') }}</button>
  </div>

  <!-- 正常态 -->
  <div v-else-if="detail" class="detail-content">
    <!-- 头部 -->
    <div class="detail-head">
      <h1 class="detail-title">
        <span>{{ selectedEntry?.name ?? selectedFile }}</span>
        <span v-if="selectedEntry" class="mem-type" :class="`mem-type-${selectedEntry.type}`">
          {{ selectedEntry.type }}
        </span>
        <span v-if="isStale" class="badge-stale">{{ t('memory.stale') }}</span>
      </h1>
      <div class="detail-actions">
        <button class="ws-btn" @click="emit('edit')">
          <span class="i-carbon-edit w-3 h-3" />
          {{ t('common.edit') }}
        </button>
        <button class="ws-btn ws-btn-danger" @click="emit('delete')">
          <span class="i-carbon-trash-can w-3 h-3" />
          {{ t('common.delete') }}
        </button>
      </div>
    </div>

    <!-- frontmatter 卡 -->
    <div class="fm-card">
      <div class="fm-title">Frontmatter</div>
      <div v-for="entry in frontmatterEntries" :key="entry.key" class="fm-row">
        <span class="fm-key">{{ entry.key }}</span>
        <span class="fm-val">{{ entry.value }}</span>
      </div>
      <div class="fm-row">
        <span class="fm-key">mtime</span>
        <span class="fm-val fm-val-mono">{{ mtimeDisplay }}</span>
      </div>
    </div>

    <!-- 正文 markdown -->
    <div
      v-if="detail.body"
      class="md-body memory-md-body"
      v-html="renderedBody"
      @click="handleBodyClick"
    />
  </div>
</template>

<style scoped>
.detail-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 200px;
}
.detail-error {
  padding: 32px 20px;
  text-align: center;
}
.detail-content {
  padding: 20px 26px;
}
.detail-head {
  margin-bottom: 16px;
}
.detail-title {
  font-size: 15px;
  font-weight: 700;
  display: flex;
  align-items: baseline;
  gap: 8px;
  flex-wrap: wrap;
}
.detail-actions {
  display: flex;
  gap: 6px;
  margin-top: 8px;
}
.ws-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  padding: 3px 10px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--card);
  cursor: pointer;
  color: var(--foreground);
  box-shadow: var(--shadow-paper);
}
.ws-btn:hover {
  background: var(--muted);
}
.ws-btn-danger {
  color: var(--destructive);
  border-color: color-mix(in oklch, var(--destructive) 40%, var(--border));
}
.ws-btn-danger:hover {
  background: color-mix(in oklch, var(--destructive) 8%, var(--card));
}

/* frontmatter 卡 */
.fm-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-paper);
  overflow: hidden;
  margin-bottom: 16px;
}
.fm-title {
  font-size: 11px;
  font-weight: 600;
  padding: 8px 12px;
  background: var(--secondary);
  border-bottom: 1px solid var(--border);
  color: var(--muted-foreground);
}
.fm-row {
  display: flex;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border);
  font-size: 11.5px;
  gap: 8px;
}
.fm-row:last-child {
  border-bottom: none;
}
.fm-key {
  width: 110px;
  flex-shrink: 0;
  color: var(--muted-foreground);
  font-family: var(--font-mono);
  font-size: 10.5px;
}
.fm-val {
  flex: 1;
  min-width: 0;
  word-break: break-word;
}
.fm-val-mono {
  font-family: var(--font-mono);
  font-size: 10.5px;
}

/* type 徽章 */
.mem-type {
  font-size: 9px;
  font-weight: 600;
  border-radius: 3px;
  padding: 1px 5px;
  white-space: nowrap;
}
.mem-type-project {
  background: var(--mem-project-bg, oklch(0.92 0.03 145));
  color: var(--mem-project, var(--primary));
}
.mem-type-feedback {
  background: var(--mem-feedback-bg, oklch(0.92 0.03 280));
  color: var(--mem-feedback, oklch(0.52 0.10 280));
}
.mem-type-user {
  background: var(--mem-user-bg, oklch(0.92 0.04 40));
  color: var(--mem-user, oklch(0.55 0.10 40));
}
.mem-type-reference {
  background: var(--mem-reference-bg, oklch(0.90 0.03 220));
  color: var(--mem-reference, oklch(0.50 0.08 220));
}
.mem-type-unknown {
  background: var(--muted);
  color: var(--muted-foreground);
}

.badge-stale {
  font-size: 8.5px;
  font-weight: 500;
  border-radius: 3px;
  padding: 0 5px;
  background: var(--amber-bg, oklch(0.92 0.04 70));
  color: var(--amber, oklch(0.62 0.14 70));
}

/* wiki-link 样式（v-html 内部，用 :deep） */
.memory-md-body :deep(.wikilink) {
  color: var(--primary);
  text-decoration: underline;
  text-decoration-style: solid;
  text-underline-offset: 2px;
  cursor: pointer;
}
.memory-md-body :deep(.wikilink:hover) {
  opacity: 0.8;
}
.memory-md-body :deep(.wikilink-broken) {
  color: var(--destructive);
  text-decoration: underline;
  text-decoration-style: dashed;
  text-underline-offset: 2px;
  cursor: default;
  opacity: 0.7;
}
</style>
