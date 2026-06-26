<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useWorkbench, setRightZoneWidth, MIN_COLUMN_WIDTH } from '@/composables/useWorkbench'
import { useRaceInput } from '@/composables/useRaceInput'
import { useProjects } from '@/composables/useProjects'
import { useConfirm } from '@/composables/useConfirm'
import { shortModel, formatTokens, type TokenUsage, type SessionSummary } from '@/types'
import WorkbenchColumnView from './WorkbenchColumn.vue'

const { activeTab, resetRaceLanes } = useWorkbench()
const { t } = useI18n()
const { projects } = useProjects()
const { confirm } = useConfirm()

async function onResetRace() {
  const ok = await confirm(t('workbench.race.resetConfirm'), t('workbench.race.reset'))
  if (!ok) return
  resetRaceLanes(activeTab.value.id)
}

const race = computed(() => activeTab.value.race!)

const {
  inputText,
  textareaRef,
  imageInput,
  slashError,
  anyStreaming,
  streamingCount,
  broadcastSend,
  stopAll,
  forkNewLane,
} = useRaceInput(activeTab)

const containerRef = ref<HTMLElement>()
const showHud = ref(false)

function getSessionSummary(sessionId: string): SessionSummary | null {
  for (const p of projects.value) {
    const s = p.sessions.find(s => s.id === sessionId)
    if (s) return s
  }
  return null
}

function cacheHitRate(t: TokenUsage): string {
  const total = t.input_tokens + t.cache_read_input_tokens + t.cache_creation_input_tokens
  return total > 0 ? Math.round(t.cache_read_input_tokens / total * 100) + '%' : '—'
}

function cacheOverallRate(t: TokenUsage): string {
  const total = t.input_tokens + t.output_tokens + t.cache_read_input_tokens + t.cache_creation_input_tokens
  return total > 0 ? Math.round(t.cache_read_input_tokens / total * 100) + '%' : '—'
}

let resizeObserver: ResizeObserver | null = null

function onWheelCapture(e: WheelEvent) {
  const el = containerRef.value
  if (!el || el.scrollWidth <= el.clientWidth) return
  if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return
  e.preventDefault()
  el.scrollLeft += e.deltaX
}

onMounted(() => {
  const el = containerRef.value
  if (!el) return
  setRightZoneWidth(el.clientWidth)
  el.addEventListener('wheel', onWheelCapture, { capture: true, passive: false })
  resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      if (entry.contentRect.width > 0) setRightZoneWidth(entry.contentRect.width)
    }
  })
  resizeObserver.observe(el)
  imageInput.attach()
})

onUnmounted(() => {
  containerRef.value?.removeEventListener('wheel', onWheelCapture, { capture: true } as EventListenerOptions)
  resizeObserver?.disconnect()
  resizeObserver = null
})

function autoResize() {
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 160) + 'px'
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    broadcastSend()
  }
}
</script>

<template>
  <div class="flex-1 min-w-0 h-full flex flex-col">
    <div class="flex-1 min-h-0 flex flex-row">
      <!-- 赛道区(横向滚动) -->
      <div ref="containerRef" class="flex-1 min-w-0 overflow-x-auto flex flex-row p-2.5 gap-2.5">
        <div
          v-for="(col, i) in activeTab.columns"
          :key="col.id"
          class="h-full relative"
          :style="{ flex: `1 0 ${MIN_COLUMN_WIDTH}px` }"
        >
          <WorkbenchColumnView :column="col" :tab-id="activeTab.id" :index="i" />

          <!-- Token HUD 覆盖层 -->
          <div
            v-if="showHud"
            class="race-hud"
          >
            <template v-if="getSessionSummary(col.sessionId)">
              <div class="hud-row font-medium">
                <span>{{ shortModel(getSessionSummary(col.sessionId)!.model ?? '') }}</span>
              </div>
              <div class="hud-divider" />
              <div class="hud-row">
                <span>input_tokens</span>
                <span>{{ formatTokens(getSessionSummary(col.sessionId)!.total_tokens.input_tokens) }}</span>
              </div>
              <div class="hud-row">
                <span>output_tokens</span>
                <span>{{ formatTokens(getSessionSummary(col.sessionId)!.total_tokens.output_tokens) }}</span>
              </div>
              <div class="hud-row">
                <span>cache_creation</span>
                <span>{{ formatTokens(getSessionSummary(col.sessionId)!.total_tokens.cache_creation_input_tokens) }}</span>
              </div>
              <div class="hud-row">
                <span>cache_read</span>
                <span>{{ formatTokens(getSessionSummary(col.sessionId)!.total_tokens.cache_read_input_tokens) }}</span>
              </div>
              <div class="hud-divider" />
              <div class="hud-row">
                <span>{{ $t('topbar.tokenTotalInput') }}</span>
                <span>{{ formatTokens(getSessionSummary(col.sessionId)!.total_tokens.input_tokens + getSessionSummary(col.sessionId)!.total_tokens.cache_creation_input_tokens + getSessionSummary(col.sessionId)!.total_tokens.cache_read_input_tokens) }}</span>
              </div>
              <div class="hud-row">
                <span>{{ $t('topbar.tokenTotalOutput') }}</span>
                <span>{{ formatTokens(getSessionSummary(col.sessionId)!.total_tokens.output_tokens) }}</span>
              </div>
              <div class="hud-row">
                <span>{{ $t('topbar.tokenCacheHitRate') }}</span>
                <span>{{ cacheHitRate(getSessionSummary(col.sessionId)!.total_tokens) }}</span>
              </div>
              <div class="hud-row">
                <span>{{ $t('topbar.tokenCacheRatio') }}</span>
                <span>{{ cacheOverallRate(getSessionSummary(col.sessionId)!.total_tokens) }}</span>
              </div>
              <div class="hud-divider" />
              <div class="hud-row font-medium">
                <span>{{ $t('topbar.tokenTotal') }}</span>
                <span>{{ formatTokens(
                  getSessionSummary(col.sessionId)!.total_tokens.input_tokens +
                  getSessionSummary(col.sessionId)!.total_tokens.output_tokens +
                  getSessionSummary(col.sessionId)!.total_tokens.cache_read_input_tokens +
                  getSessionSummary(col.sessionId)!.total_tokens.cache_creation_input_tokens
                ) }}</span>
              </div>
            </template>
          </div>
        </div>
      </div>

      <!-- 右侧工具栏 -->
      <div class="shrink-0 w-10 flex flex-col items-center gap-2 py-2.5 border-l border-border bg-background">
        <button
          class="icon-btn icon-btn-lg"
          :class="showHud && 'icon-btn-active'"
          v-tooltip="$t('workbench.race.tokenHud')"
          @click="showHud = !showHud"
        >
          <span class="i-carbon-dashboard w-3.5 h-3.5" />
        </button>
        <button
          class="icon-btn icon-btn-lg"
          v-tooltip="$t('workbench.race.reset')"
          @click="onResetRace"
        >
          <span class="i-carbon-reset w-3.5 h-3.5" />
        </button>
        <button
          class="icon-btn icon-btn-lg icon-btn-dashed flex-1"
          v-tooltip="t('workbench.race.addLane')"
          @click="forkNewLane"
        >
          <span class="i-carbon-add w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <!-- 输入区 -->
    <div class="px-4 py-3 border-t border-border shrink-0">
      <div v-if="slashError" class="mb-1 text-xs text-destructive">
        {{ slashError }}
      </div>

      <div v-if="imageInput.images.value.length" class="mb-2 flex gap-2 flex-wrap">
        <div v-for="img in imageInput.images.value" :key="img.id" class="relative w-14 h-14 rounded border border-border overflow-hidden group">
          <img :src="img.dataUrl" class="w-full h-full object-cover" />
          <button
            class="absolute top-0 right-0 w-4 h-4 rounded-bl bg-destructive/80 text-destructive-foreground flex items-center justify-center text-2.5 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
            @click="imageInput.removeImage(img.id)"
          >&times;</button>
        </div>
      </div>

      <div v-if="imageInput.lastError.value" class="mb-1 text-xs text-destructive">
        {{ imageInput.lastError.value.message }}
      </div>

      <div class="flex items-center gap-2">
        <textarea
          ref="textareaRef"
          v-model="inputText"
          :placeholder="t('workbench.race.sharedInput')"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-popover border border-border text-foreground placeholder-muted-foreground resize-none focus:outline-none focus:border-ring transition-colors"
          @keydown="onInputKeydown"
          @input="autoResize"
        />

        <button
          v-if="anyStreaming && !inputText.trim() && !imageInput.images.value.length"
          class="px-3 py-2 text-xs rounded-md bg-accent text-accent-foreground hover:shadow-paper transition-shadow shrink-0"
          @click="stopAll"
        >
          {{ t('workbench.race.stopAll') }}
          <span v-if="streamingCount > 0" class="ml-1 opacity-60">{{ streamingCount }}/{{ race.lanes.length }}</span>
        </button>

        <button
          v-else
          :disabled="!inputText.trim() && !imageInput.images.value.length"
          class="px-3 py-2 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow shrink-0 disabled:opacity-30 disabled:cursor-not-allowed"
          @click="broadcastSend"
        >
          {{ t('common.send') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.race-hud {
  position: absolute;
  top: 40px;
  right: 6px;
  z-index: 20;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--foreground);
  color: var(--background);
  opacity: 0.75;
  font-size: 11px;
  line-height: 1.6;
  font-variant-numeric: tabular-nums;
  pointer-events: none;
  min-width: 130px;
}
.hud-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.hud-sep {
  opacity: 0.5;
}
.hud-divider {
  border-top: 1px solid currentColor;
  opacity: 0.2;
  margin: 2px 0;
}
</style>
