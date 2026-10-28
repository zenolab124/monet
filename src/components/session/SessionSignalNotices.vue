<script setup lang="ts">
import { computed, toRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { dismissNotice, useSessionSignals, type SessionNotice } from '@/composables/useSessionSignals'
import { useAutoResume } from '@/composables/useAutoResume'

/** 上下文达到该规模才提示缓存重建与"先压缩" */
const LARGE_CONTEXT_TOKENS = 100_000
/** 闲置超过该时长视为缓存已过期（CLI 使用 1 小时缓存） */
const CACHE_TTL_MS = 60 * 60 * 1000

const props = defineProps<{
  sessionId: string
  contextTokens: number | null
  lastActivityAt: number | null
  composerFocused: boolean
  busy: boolean
}>()

const emit = defineEmits<{ (e: 'compact'): void }>()

const { t } = useI18n()
const sessionIdRef = toRef(props, 'sessionId')
const signals = useSessionSignals(sessionIdRef)
const autoResume = useAutoResume(sessionIdRef)

const rateLimit = computed(() => {
  const value = signals.value?.rateLimit
  return value && value.status !== 'allowed' ? value : null
})

function windowLabel(type: string | null): string {
  if (type === 'five_hour') return t('session.signals.windowFiveHour')
  if (type === 'seven_day') return t('session.signals.windowSevenDay')
  return type ?? ''
}

function formatReset(ms: number): string {
  const date = new Date(ms)
  const now = new Date()
  const time = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  if (date.toDateString() === now.toDateString()) return time
  const tomorrow = new Date(now)
  tomorrow.setDate(now.getDate() + 1)
  if (date.toDateString() === tomorrow.toDateString()) return t('session.signals.tomorrow', { time })
  return date.toLocaleString([], { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

const rateText = computed(() => {
  const value = rateLimit.value
  if (!value) return ''
  const window = windowLabel(value.rateLimitType)
  const parts = [
    value.status === 'rejected'
      ? t('session.signals.rateRejected', { window })
      : t('session.signals.rateWarning', { window }),
  ]
  if (value.resetsAtMs) parts.push(t('session.signals.resetsAt', { time: formatReset(value.resetsAtMs) }))
  if (value.isUsingOverage) parts.push(t('session.signals.overage'))
  return parts.join(' · ')
})

const isFiveHour = computed(() => rateLimit.value?.rateLimitType === 'five_hour')
const largeContext = computed(() => (props.contextTokens ?? 0) >= LARGE_CONTEXT_TOKENS)
const showCompactInBanner = computed(() =>
  rateLimit.value?.status === 'allowed_warning' && isFiveHour.value && largeContext.value,
)

const showCacheHint = computed(() =>
  props.composerFocused
  && largeContext.value
  && props.lastActivityAt !== null
  && Date.now() - props.lastActivityAt > CACHE_TTL_MS,
)

const tokensLabel = computed(() => {
  const tokens = props.contextTokens ?? 0
  return tokens >= 10_000 ? `${Math.round(tokens / 1000)}k` : String(tokens)
})

const liveText = computed(() => {
  const live = signals.value?.liveStatus
  if (!live) return ''
  if (live.kind === 'compacting') return t('session.signals.compacting')
  return live.attempt
    ? t('session.signals.retrying', { attempt: live.attempt })
    : t('session.signals.retryingNoCount')
})

function noticeText(notice: SessionNotice): string {
  const p = notice.params
  switch (notice.kind) {
    case 'permission_mode_failed':
      return p.error === 'timeout'
        ? t('session.signals.permissionModeTimeout')
        : t('session.signals.permissionModeFailed', { error: String(p.error ?? '') })
    case 'unsupported_request': {
      const counts = (p.counts ?? {}) as Record<string, number>
      const list = Object.entries(counts).map(([name, count]) => `${name} ×${count}`).join('、')
      return t('session.signals.unsupportedRequests', { list })
    }
    case 'init_issues': {
      const items = (p.items ?? []) as Array<{ name: string; message: string }>
      const detail = items.map(item => item.name ? `${item.name}: ${item.message}` : item.message).join('；')
      return `${t('session.signals.initIssues')} ${detail}`
    }
    case 'model_fallback':
      if (p.from && p.to) return t('session.signals.modelFallback', { from: String(p.from), to: String(p.to) })
      if (p.to) return t('session.signals.modelFallbackTo', { to: String(p.to) })
      if (p.from) return t('session.signals.modelFallbackFrom', { from: String(p.from) })
      return t('session.signals.modelFallbackUnknown')
    case 'permission_denied':
      return p.tool
        ? t('session.signals.permissionDenied', { tool: String(p.tool) })
        : t('session.signals.permissionDeniedUnknown')
    case 'auto_resume_expired':
      return t('session.signals.autoResumeExpired')
    case 'auto_resume_still_limited':
      return t('session.signals.autoResumeStillLimited')
    case 'auto_resume_sent':
      return t('session.signals.autoResumeSent')
  }
  return ''
}

function noticeIcon(notice: SessionNotice): string {
  switch (notice.kind) {
    case 'permission_mode_failed':
    case 'init_issues':
    case 'auto_resume_still_limited':
      return 'i-carbon-warning-alt text-destructive'
    case 'model_fallback':
      return 'i-carbon-shuffle text-muted-foreground'
    case 'permission_denied':
      return 'i-carbon-locked text-muted-foreground'
    default:
      return 'i-carbon-information text-muted-foreground'
  }
}

function toggleAutoResume(event: Event) {
  autoResume.setEnabled((event.target as HTMLInputElement).checked, signals.value?.rateLimit ?? null)
}
</script>

<template>
  <div v-if="liveText" class="mb-1 text-xs text-muted-foreground flex items-center gap-1.5">
    <span class="w-1.5 h-1.5 rounded-full bg-claude animate-pulse shrink-0" />
    {{ liveText }}
  </div>

  <div
    v-if="rateLimit"
    class="mb-1 rounded-md border px-2.5 py-1.5 text-xs flex items-center gap-2 flex-wrap"
    :class="rateLimit.status === 'rejected' ? 'border-destructive/30 bg-destructive/5' : 'border-amber-500/30 bg-amber-500/5'"
  >
    <span
      class="w-3.5 h-3.5 shrink-0"
      :class="rateLimit.status === 'rejected' ? 'i-carbon-error text-destructive' : 'i-carbon-warning text-amber-600 dark:text-amber-400'"
    />
    <span class="text-foreground/90">{{ rateText }}</span>
    <span class="flex-1" />
    <button
      v-if="showCompactInBanner"
      class="px-2 py-0.5 rounded border border-border hover:bg-accent transition-colors disabled:opacity-50"
      :disabled="busy"
      :title="busy ? t('session.signals.compactAfterTurn') : undefined"
      @click="emit('compact')"
    >{{ t('session.signals.compactFirst') }}</button>
    <label v-if="isFiveHour" class="flex items-center gap-1 cursor-pointer select-none text-muted-foreground">
      <input type="checkbox" class="accent-current" :checked="autoResume.enabled.value" @change="toggleAutoResume">
      {{ t('session.signals.autoResume') }}
    </label>
    <span v-if="autoResume.plan.value?.state === 'scheduled'" class="w-full text-muted-foreground">
      {{ t('session.signals.autoResumeScheduled', { time: formatReset(autoResume.plan.value.dueAt) }) }}
    </span>
  </div>

  <div v-if="showCacheHint" class="mb-1 text-xs text-muted-foreground flex items-center gap-1.5 flex-wrap">
    <span class="i-carbon-time w-3 h-3 shrink-0" />
    {{ t('session.signals.cacheRebuild', { tokens: tokensLabel }) }}
    <button
      class="px-2 py-0.5 rounded border border-border hover:bg-accent transition-colors disabled:opacity-50"
      :disabled="busy"
      :title="busy ? t('session.signals.compactAfterTurn') : undefined"
      @mousedown.prevent
      @click="emit('compact')"
    >{{ t('session.signals.compactFirst') }}</button>
  </div>

  <div
    v-for="notice in signals?.notices ?? []"
    :key="notice.id"
    class="mb-1 text-xs flex items-start gap-1.5"
  >
    <span class="w-3.5 h-3.5 shrink-0 mt-px" :class="noticeIcon(notice)" />
    <span class="flex-1 text-foreground/85 break-words">{{ noticeText(notice) }}</span>
    <button
      class="i-carbon-close w-3.5 h-3.5 shrink-0 text-muted-foreground hover:text-foreground"
      :aria-label="t('common.close')"
      @click="dismissNotice(sessionId, notice.id)"
    />
  </div>
</template>
