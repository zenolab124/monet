<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { shortId } from '@/types'

const { t } = useI18n()

interface HookEvent {
  subtype: 'hook_started' | 'hook_response'
  hook_name: string
  hook_event: string
  output?: string
  exit_code?: number
}

const props = defineProps<{
  sessionId: string
  resumed: boolean
  cwd: string
  model: string | null
  effort: string | null
  features: string[]
  hookEvents: HookEvent[]
}>()

function cleanHookOutput(raw?: string): string {
  if (!raw) return ''
  // hookSpecificOutput JSON 是给 CLI 内部注入用的，不展示
  const idx = raw.indexOf('{"hookSpecificOutput"')
  return (idx >= 0 ? raw.slice(0, idx) : raw).trim()
}

const hookOutputs = computed(() =>
  props.hookEvents
    .filter(e => e.subtype === 'hook_response')
    .map(e => cleanHookOutput(e.output))
    .filter(Boolean),
)
</script>

<template>
  <div class="session-banner">
    <div class="banner-header">
      <span class="i-carbon-information w-3.5 h-3.5" />
      <span class="banner-title">{{ t('session.bannerTitle') }}</span>
    </div>
    <div class="banner-grid">
      <span class="banner-label">{{ t('session.bannerSession') }}</span>
      <span class="banner-value">
        <code>{{ shortId(sessionId) }}</code>
        <span class="banner-tag">{{ resumed ? t('session.bannerResumed') : t('session.bannerNew') }}</span>
      </span>

      <span class="banner-label">{{ t('session.bannerCwd') }}</span>
      <span class="banner-value banner-mono">{{ cwd }}</span>

      <template v-if="model">
        <span class="banner-label">{{ t('session.bannerModel') }}</span>
        <span class="banner-value">{{ model }}</span>
      </template>

      <template v-if="effort">
        <span class="banner-label">{{ t('session.bannerEffort') }}</span>
        <span class="banner-value">{{ effort }}</span>
      </template>

      <template v-if="features.length">
        <span class="banner-label">{{ t('session.bannerFeatures') }}</span>
        <span class="banner-value">
          <span v-for="f in features" :key="f" class="banner-tag feature">{{ f }}</span>
        </span>
      </template>

    </div>
    <div v-for="(h, i) in hookOutputs" :key="i" class="hook-output">{{ h }}</div>
  </div>
</template>

<style scoped>
.session-banner {
  border: 1px dashed var(--border);
  border-radius: var(--radius);
  padding: 8px 12px;
  font-size: 11px;
  color: var(--muted-foreground);
  user-select: none;
}
.banner-header {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 6px;
  font-weight: 500;
  color: var(--foreground);
}
.banner-title { font-size: 11px; }
.banner-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 2px 12px;
  align-items: baseline;
}
.banner-label {
  font-weight: 500;
  white-space: nowrap;
}
.banner-value {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.banner-value code {
  font-family: var(--font-mono);
  font-size: 10px;
}
.banner-mono {
  font-family: var(--font-mono);
  font-size: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.banner-tag {
  display: inline-block;
  padding: 0 5px;
  border-radius: 3px;
  font-size: 10px;
  background: var(--secondary);
  border: 1px solid var(--border);
  line-height: 1.6;
}
.banner-tag.feature {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: transparent;
}
.hook-output {
  margin-top: 4px;
  font-size: 11px;
}
</style>
