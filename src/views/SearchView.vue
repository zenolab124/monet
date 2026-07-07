<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSearch, type SearchHit } from '@/composables/useSearch'
import { useProjects } from '@/composables/useProjects'
import { useUiState } from '@/composables/useUiState'

const { t } = useI18n()
const {
  query, days30, titleOnly, projectFilter,
  result, searching, searchError, indexStatus,
  runSearch, refreshStatus, goToHit,
} = useSearch()
const { projects } = useProjects()
const { activeSection } = useUiState()

const inputEl = ref<HTMLInputElement | null>(null)
const projectMenuOpen = ref(false)

// 进域时聚焦搜索框 + 刷新索引状态
watch(activeSection, async (s) => {
  if (s === 'search') {
    refreshStatus()
    await nextTick()
    inputEl.value?.focus()
  }
}, { immediate: true })

/** projectId → 展示名（display_path 末段）*/
const projectNames = computed<Record<string, string>>(() => {
  const map: Record<string, string> = {}
  for (const p of projects.value) {
    map[p.id] = p.display_path.split('/').filter(Boolean).pop() ?? p.id
  }
  return map
})

const currentProjectLabel = computed(() =>
  projectFilter.value
    ? (projectNames.value[projectFilter.value] ?? projectFilter.value)
    : t('search.allProjects'),
)

function pickProject(id: string | null) {
  projectFilter.value = id
  projectMenuOpen.value = false
}

/** 查询词列表（高亮用）*/
const terms = computed(() =>
  query.value.trim().split(/\s+/).filter(Boolean),
)

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** KWIC 高亮：全部词合成单个 alternation 正则一次替换，避免嵌套污染 */
function highlight(text: string): string {
  const html = escapeHtml(text)
  if (!terms.value.length) return html
  const pattern = terms.value.map(w => escapeRegExp(escapeHtml(w))).join('|')
  try {
    return html.replace(new RegExp(`(${pattern})`, 'gi'), '<mark>$1</mark>')
  } catch (_) {
    return html
  }
}

/** 相对时间（秒级 epoch）*/
function relativeTime(epochSecs: number): string {
  const diff = Date.now() / 1000 - epochSecs
  if (diff < 60) return t('time.justNow')
  if (diff < 3600) return t('time.minutesAgo', { n: Math.floor(diff / 60) })
  if (diff < 86400) return t('time.hoursAgo', { n: Math.floor(diff / 3600) })
  if (diff < 172800) return t('time.yesterday')
  if (diff < 7 * 86400) return t('time.daysAgo', { n: Math.floor(diff / 86400) })
  return new Date(epochSecs * 1000).toLocaleDateString()
}

function onHitClick(hit: SearchHit) {
  // 默认定位到首个命中片段
  goToHit(hit, hit.snippets[0]?.uuid ?? null)
}
</script>

<template>
  <main class="flex-1 min-w-0 overflow-y-auto bg-background" @click="projectMenuOpen = false">
    <div class="max-w-180 mx-auto px-6 py-6">
      <!-- 页头：标题 + 索引状态 -->
      <div class="flex items-baseline gap-3 mb-4">
        <h1 class="text-lg font-semibold text-foreground">{{ t('activity.search') }}</h1>
        <span v-if="indexStatus" class="text-xs text-muted-foreground">
          {{ indexStatus.state === 'ready'
            ? t('search.indexed', { n: indexStatus.indexedSessions })
            : t('search.building') }}
        </span>
      </div>

      <!-- 搜索框 -->
      <div class="flex items-center gap-2 px-3 py-2 mb-2.5 bg-card border border-input rounded focus-within:border-ring focus-within:shadow-paper transition-colors">
        <span class="i-carbon-search w-4 h-4 text-muted-foreground shrink-0" />
        <input
          ref="inputEl"
          v-model="query"
          type="text"
          :placeholder="t('search.placeholder')"
          class="flex-1 min-w-0 bg-transparent outline-none border-none text-sm text-foreground placeholder:text-muted-foreground"
          @keydown.enter="runSearch"
        />
        <span v-if="searching" class="i-carbon-renew w-3.5 h-3.5 text-muted-foreground animate-spin shrink-0" />
      </div>

      <!-- 过滤 chips -->
      <div class="flex items-center gap-1.5 mb-4">
        <div class="relative">
          <button
            class="px-2 py-0.5 text-xs rounded transition-colors flex items-center gap-1"
            :class="projectFilter ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground'"
            @click.stop="projectMenuOpen = !projectMenuOpen"
          >
            {{ currentProjectLabel }}
            <span class="i-carbon-chevron-down w-3 h-3" />
          </button>
          <div
            v-if="projectMenuOpen"
            class="absolute left-0 top-full mt-1 z-20 max-h-72 w-56 overflow-y-auto bg-card border border-border rounded-md shadow-paper-lifted py-1"
            @click.stop
          >
            <button
              class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted transition-colors"
              :class="projectFilter === null ? 'text-accent' : 'text-foreground'"
              @click="pickProject(null)"
            >{{ t('search.allProjects') }}</button>
            <button
              v-for="p in projects"
              :key="p.id"
              class="w-full text-left px-3 py-1.5 text-xs truncate hover:bg-muted transition-colors"
              :class="projectFilter === p.id ? 'text-accent' : 'text-foreground'"
              :title="p.display_path"
              @click="pickProject(p.id)"
            >{{ projectNames[p.id] }}</button>
          </div>
        </div>
        <button
          class="px-2 py-0.5 text-xs rounded transition-colors"
          :class="days30 ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground'"
          @click="days30 = !days30"
        >{{ t('search.days30') }}</button>
        <button
          class="px-2 py-0.5 text-xs rounded transition-colors"
          :class="titleOnly ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground'"
          @click="titleOnly = !titleOnly"
        >{{ t('search.titleOnly') }}</button>
        <span v-if="result && !searching" class="ml-auto text-xs text-muted-foreground">
          {{ t('search.resultStats', { n: result.totalHits, ms: result.elapsedMs }) }}
        </span>
      </div>

      <!-- 错误态 -->
      <div v-if="searchError" class="text-sm text-destructive py-8 text-center">
        {{ searchError }}
        <button class="block mx-auto mt-2 text-xs text-accent hover:underline" @click="runSearch">
          {{ t('common.retry') }}
        </button>
      </div>

      <!-- 空态：未输入 -->
      <div v-else-if="!query.trim()" class="text-sm text-muted-foreground text-center py-16">
        {{ t('search.emptyHint') }}
      </div>

      <!-- 无结果 -->
      <div v-else-if="result && result.hits.length === 0 && !searching" class="text-sm text-muted-foreground text-center py-16">
        {{ t('search.noResults') }}
      </div>

      <!-- 结果列表 -->
      <div v-else-if="result" class="flex flex-col gap-2">
        <div
          v-for="hit in result.hits"
          :key="hit.sessionId"
          class="bg-card border border-border rounded shadow-paper hover:shadow-paper-lifted px-3.5 py-2.5 cursor-pointer transition-shadow"
          @click="onHitClick(hit)"
        >
          <div class="flex items-baseline gap-2 min-w-0">
            <!-- eslint-disable-next-line vue/no-v-html -->
            <span class="text-sm font-medium text-foreground truncate" v-html="highlight(hit.title ?? t('search.untitled'))" />
            <span v-if="hit.totalMatches > 0" class="text-xs text-muted-foreground shrink-0">
              {{ t('search.matchCount', { n: hit.totalMatches }) }}
            </span>
          </div>
          <div class="mt-1.5 flex flex-col gap-1">
            <div
              v-for="(sn, si) in hit.snippets"
              :key="sn.uuid ?? si"
              class="flex items-start gap-1.5 text-xs leading-relaxed text-muted-foreground hover:text-foreground transition-colors"
              @click.stop="goToHit(hit, sn.uuid)"
            >
              <span
                class="w-3.5 h-3.5 mt-0.5 shrink-0"
                :class="sn.role === 0 ? 'i-carbon-user' : 'i-carbon-bot'"
              />
              <!-- eslint-disable-next-line vue/no-v-html -->
              <span class="min-w-0 snippet" v-html="highlight(sn.text)" />
            </div>
          </div>
          <div class="mt-1.5 flex items-center gap-2.5 text-xs text-muted-foreground">
            <span class="flex items-center gap-1">
              <span class="i-carbon-folder w-3 h-3" />
              {{ projectNames[hit.projectId] ?? hit.projectId }}
            </span>
            <span>{{ relativeTime(hit.lastModified) }}</span>
          </div>
        </div>
      </div>
    </div>
  </main>
</template>

<style scoped>
:deep(mark) {
  background: var(--secondary);
  color: var(--accent);
  font-weight: 600;
  padding: 0 2px;
  border-radius: 2px;
}
.snippet {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
