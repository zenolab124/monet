<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { DragDropProvider } from '@dnd-kit/vue'
import { move } from '@dnd-kit/helpers'
import SortableChainItem from '@/components/settings/SortableChainItem.vue'
import {
  useChannels,
  refreshChannels,
  OFFICIAL_CHANNEL_ID,
  type ChannelInfo,
} from '@/composables/useChannels'
import { useUiState } from '@/composables/useUiState'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import { useLocale } from '@/composables/useLocale'
import { useHomeStats } from '@/composables/useHomeStats'
import ChannelForm from '@/components/settings/ChannelForm.vue'
import DiagnosisCard from '@/components/home/DiagnosisCard.vue'
import AgentIframeDemo from '@/components/settings/AgentIframeDemo.vue'
import ClaudeCodeSettings from '@/components/settings/ClaudeCodeSettings.vue'
import { useAppDefaults } from '@/composables/useAppDefaults'
import { useHtmlVisual } from '@/features'
import { useTheme } from '@/composables/useTheme'

const { t } = useI18n()
const {
  channels, sessionChain, agentChain, probeResults, probing,
  revealedTokens, revealToken, hideToken,
  deleteChannel, setChannelEnabled, setSessionChain, setAgentChain, revealChannelsDir,
  probeChannel, probeAllChannels,
} = useChannels()
const { activeSection } = useUiState()
const { diag, diagLoading, diagError, diagAt, retryDiag, ensureLoaded } = useHomeStats()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()
const {
  locale, availableLocales, switchLocale,
  translating, translateError, parseLanguageIntent, translateLocale, deleteLocale, isBuiltin,
} = useLocale()

const { enabled: htmlVisualEnabled } = useHtmlVisual()
const { config: themeConfig, themes: themeList, setLightTheme, setDarkTheme } = useTheme()

const agentToggles = ref<Record<string, boolean>>({})
const agentKeys = [
  { key: 'title', label: 'settings.agentTitle', desc: 'settings.agentTitleDesc' },
  { key: 'permission_hint', label: 'settings.agentPermissionHint', desc: 'settings.agentPermissionHintDesc' },
  { key: 'settings_explain', label: 'settings.agentSettingsExplain', desc: 'settings.agentSettingsExplainDesc' },
  { key: 'cron_parse', label: 'settings.agentCronParse', desc: 'settings.agentCronParseDesc' },
  { key: 'tags', label: 'settings.agentTags', desc: 'settings.agentTagsDesc' },
  { key: 'summary', label: 'settings.agentSummary', desc: 'settings.agentSummaryDesc' },
  { key: 'translate', label: 'settings.agentTranslate', desc: 'settings.agentTranslateDesc' },
]

async function loadAgentToggles() {
  agentToggles.value = await invoke<Record<string, boolean>>('get_agent_toggles')
}

function isAgentEnabled(key: string) {
  return agentToggles.value[key] ?? true
}

async function toggleAgent(key: string) {
  const next = !isAgentEnabled(key)
  agentToggles.value = { ...agentToggles.value, [key]: next }
  await invoke('set_agent_toggle', { key, enabled: next })
}

const showTranslateForm = ref(false)
const customLangInput = ref('')

async function onCustomTranslate() {
  const input = customLangInput.value.trim()
  if (!input) return
  const intent = await parseLanguageIntent(input)
  if (!intent || intent.error) {
    translateError.value = intent?.error || t('settings.langNotRecognized')
    return
  }
  if (intent.code in availableLocales.value) {
    switchLocale(intent.code)
    return
  }
  const ok = await translateLocale(intent.code, intent.name, intent.native)
  if (ok) {
    customLangInput.value = ''
    notifyTransient(t('settings.translateSuccess'))
  }
}

type Tab = 'general' | 'channels' | 'models' | 'agent' | 'claude-code' | 'lab' | 'diag'
const activeTab = ref<Tab>('general')

const editing = ref<'new' | ChannelInfo | null>(null)

const { appDefaults, setDefaultEffort } = useAppDefaults()
const rcEnabled = ref(true)
const wakePolicy = ref('passive')

async function loadWakePolicy() {
  try {
    wakePolicy.value = await invoke<string>('get_routine_wake_policy')
  } catch {}
}
async function setWakePolicy(policy: string) {
  wakePolicy.value = policy
  try {
    await invoke('set_routine_wake_policy', { policy })
  } catch (e) {
    wakePolicy.value = policy === 'active' ? 'passive' : 'active'
  }
}

// 模型 — 暂用本地状态
const advisorMain = ref('claude-sonnet-4-6')
const advisorModel = ref('claude-fable-5')
const hideCreditsModels = ref(false)
const autoDetectModels = ref(false)

onMounted(() => { refreshChannels(); loadAgentToggles(); loadWakePolicy() })

watch(activeSection, (s) => {
  if (s === 'settings') {
    refreshChannels().then(() => probeAllChannels())
    ensureLoaded()
  }
})

async function onDelete(ch: ChannelInfo) {
  const ok = await confirm(
    t('settings.deleteChannelConfirm', { name: ch.name, id: ch.id }),
    t('common.delete'),
  )
  if (!ok) return
  try {
    await deleteChannel(ch.id)
    if (editing.value !== 'new' && editing.value?.id === ch.id) editing.value = null
    notifyTransient(t('settings.channelDeleted'))
  } catch (e) {
    notifyTransient(t('settings.deleteFailed'), String(e))
  }
}

// ---- 渠道链拖拽排序（@dnd-kit/vue） ----
function channelById(id: string): ChannelInfo {
  return channels.value.find(c => c.id === id) || {
    id, name: id === OFFICIAL_CHANNEL_ID ? t('channel.official') : id,
    enabled: true, valid: true, note: null, baseUrl: null,
    authTokenMasked: null, extraEnvKeys: [],
  } as ChannelInfo
}

const sessionPrefixed = ref<string[]>([])
const agentPrefixed = ref<string[]>([])

watch(sessionChain, (ids) => { sessionPrefixed.value = ids.map(id => `session:${id}`) }, { immediate: true })
watch(agentChain, (ids) => { agentPrefixed.value = ids.map(id => `agent:${id}`) }, { immediate: true })

function stripPrefix(prefixedId: string): string {
  const idx = prefixedId.indexOf(':')
  return idx >= 0 ? prefixedId.slice(idx + 1) : prefixedId
}

function onDragEnd(event: any) {
  const chainMap: Record<string, string[]> = {
    session: [...sessionPrefixed.value],
    agent: [...agentPrefixed.value],
  }
  const updated = move(chainMap, event)

  const newSession = (updated.session ?? []).map(stripPrefix)
  const newAgent = (updated.agent ?? []).map(stripPrefix)

  if (JSON.stringify(newSession) !== JSON.stringify(sessionChain.value)) {
    sessionChain.value = newSession
    setSessionChain(newSession)
  }
  if (JSON.stringify(newAgent) !== JSON.stringify(agentChain.value)) {
    agentChain.value = newAgent
    setAgentChain(newAgent)
  }
}

async function onReveal() {
  try {
    await revealChannelsDir()
  } catch (e) {
    notifyTransient(t('settings.openDirFailed'), String(e))
  }
}

function onSaved() {
  editing.value = null
  notifyTransient(t('settings.channelSaved'))
}
</script>

<template>
  <div class="h-full p-2.5">
    <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">
    <!-- 侧栏导航 -->
    <nav class="side-nav">
      <h1 class="side-title">
        <span class="i-carbon-settings w-4 h-4 opacity-70" />{{ $t('settings.title') }}
      </h1>
      <button :class="['side-item', { active: activeTab === 'general' }]" @click="activeTab = 'general'">
        <span class="i-carbon-settings-adjust w-3.5 h-3.5" />{{ $t('settings.general') }}
      </button>
      <button :class="['side-item', { active: activeTab === 'channels' }]" @click="activeTab = 'channels'">
        <span class="i-carbon-connect w-3.5 h-3.5" />{{ $t('settings.channels') }}
      </button>
      <button :class="['side-item', { active: activeTab === 'models' }]" @click="activeTab = 'models'">
        <span class="i-carbon-bot w-3.5 h-3.5" />{{ $t('settings.models') }}
      </button>
      <button :class="['side-item', { active: activeTab === 'agent' }]" @click="activeTab = 'agent'">
        <span class="i-carbon-machine-learning w-3.5 h-3.5" />{{ $t('settings.agent') }}
      </button>
      <button :class="['side-item', { active: activeTab === 'claude-code' }]" @click="activeTab = 'claude-code'">
        <span class="i-carbon-json w-3.5 h-3.5" />Claude Code
      </button>
      <button :class="['side-item', { active: activeTab === 'lab' }]" @click="activeTab = 'lab'">
        <span class="i-carbon-chemistry w-3.5 h-3.5" />{{ $t('settings.lab') }}
      </button>
      <button :class="['side-item', { active: activeTab === 'diag' }]" @click="activeTab = 'diag'">
        <span class="i-carbon-debug w-3.5 h-3.5" />{{ $t('settings.diagnostics') }}
      </button>
    </nav>

    <!-- 内容区 -->
    <div :class="['flex-1 min-w-0', activeTab === 'claude-code' ? 'flex flex-col overflow-hidden' : 'overflow-y-auto']">
      <div :class="['settings-body', { 'flex-1 min-h-0 flex flex-col': activeTab === 'claude-code' }]">

        <!-- ====== 常规 ====== -->
        <section v-show="activeTab === 'general'">
          <h2 class="section-title">{{ $t('settings.general') }}</h2>
          <div class="settings-grid">
            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.themeLight') }}</div>
              <select
                :value="themeConfig.lightTheme"
                class="form-select w-full"
                @change="setLightTheme(($event.target as HTMLSelectElement).value)"
              >
                <option v-for="t in themeList" :key="t.id" :value="t.id">
                  {{ $t(t.labelKey) }}
                </option>
              </select>
              <div class="setting-hint">{{ $t('settings.themeLightHint') }}</div>
            </div>
            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.themeDark') }}</div>
              <select
                :value="themeConfig.darkTheme"
                class="form-select w-full"
                @change="setDarkTheme(($event.target as HTMLSelectElement).value)"
              >
                <option v-for="t in themeList" :key="t.id" :value="t.id">
                  {{ $t(t.labelKey) }}
                </option>
              </select>
              <div class="setting-hint">{{ $t('settings.themeDarkHint') }}</div>
            </div>
            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.language') }}</div>
              <select
                :value="locale"
                class="form-select w-full"
                @change="switchLocale(($event.target as HTMLSelectElement).value)"
              >
                <option
                  v-for="(meta, code) in availableLocales"
                  :key="code"
                  :value="code"
                >
                  {{ meta.nativeLabel }}
                </option>
              </select>
              <!-- AI 翻译 -->
              <div class="translate-zone">
                <div
                  v-for="(meta, code) in availableLocales"
                  :key="code"
                  class="flex items-center gap-2 text-xs"
                >
                  <template v-if="!isBuiltin(String(code))">
                    <span class="font-medium">{{ meta.nativeLabel }}</span>
                    <span class="text-muted-foreground">({{ code }})</span>
                    <button
                      class="ml-auto p-0.5 text-muted-foreground hover:text-destructive transition-colors"
                      :title="$t('common.delete')"
                      @click="deleteLocale(String(code))"
                    >
                      <span class="i-carbon-close w-3 h-3" />
                    </button>
                  </template>
                </div>
                <button
                  v-if="!showTranslateForm"
                  class="text-xs text-primary hover:underline"
                  @click="showTranslateForm = true"
                >
                  {{ $t('settings.addLanguage') }}
                </button>
                <div v-if="showTranslateForm" class="translate-form">
                  <div class="flex gap-2">
                    <input
                      v-model="customLangInput"
                      class="form-input flex-1"
                      :placeholder="$t('settings.customLangPlaceholder')"
                      :disabled="translating"
                      @keydown.enter="onCustomTranslate"
                    />
                    <button
                      class="px-2 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper disabled:opacity-50 transition"
                      :disabled="translating || !customLangInput.trim()"
                      @click="onCustomTranslate"
                    >
                      {{ $t('settings.startTranslate') }}
                    </button>
                    <button
                      class="px-1.5 py-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                      :disabled="translating"
                      @click="showTranslateForm = false"
                    >
                      {{ $t('common.cancel') }}
                    </button>
                  </div>
                  <p v-if="translating" class="text-[11px] text-muted-foreground mt-1.5">
                    <span class="i-carbon-rotate inline-block w-3 h-3 animate-spin mr-1" />{{ $t('settings.translating') }}
                  </p>
                  <p v-if="translateError" class="text-[11px] text-destructive mt-1">{{ translateError }}</p>
                </div>
              </div>
            </div>

            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.defaultEffort') }}</div>
              <select
                :value="appDefaults.effort ?? 'cli'"
                class="form-select w-full"
                @change="setDefaultEffort(($event.target as HTMLSelectElement).value === 'cli' ? null : ($event.target as HTMLSelectElement).value as any)"
              >
                <option value="cli">{{ $t('settings.followCli') }}</option>
                <option value="low">Low</option>
                <option value="medium">Medium</option>
                <option value="high">High</option>
                <option value="xhigh">xHigh</option>
                <option value="max">Max</option>
                <option value="ultracode">Ultracode</option>
              </select>
              <div class="setting-hint">{{ $t('settings.defaultEffortHint') }}</div>
            </div>

            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.remoteControl') }}</div>
              <div class="flex items-center gap-2.5">
                <button
                  :class="['form-toggle', { on: rcEnabled }]"
                  @click="rcEnabled = !rcEnabled"
                >
                  <span class="form-toggle-knob" />
                </button>
                <span class="text-[11px] text-muted-foreground">{{ $t('settings.remoteControlSub') }}</span>
              </div>
              <div class="setting-hint">{{ $t('settings.remoteControlHint') }}</div>
            </div>

            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.routineWake') }}</div>
              <div class="flex flex-col gap-1.5">
                <label class="flex items-center gap-2 cursor-pointer text-[12px]">
                  <input
                    type="radio"
                    name="wake-policy"
                    value="passive"
                    :checked="wakePolicy === 'passive'"
                    class="accent-primary"
                    @change="setWakePolicy('passive')"
                  />
                  {{ $t('settings.routineWakePassive') }}
                </label>
                <label class="flex items-center gap-2 cursor-pointer text-[12px]">
                  <input
                    type="radio"
                    name="wake-policy"
                    value="active"
                    :checked="wakePolicy === 'active'"
                    class="accent-primary"
                    @change="setWakePolicy('active')"
                  />
                  {{ $t('settings.routineWakeActive') }}
                </label>
                <span v-if="wakePolicy === 'active'" class="text-[11px] text-muted-foreground ml-5">{{ $t('settings.routineWakeActiveSub') }}</span>
              </div>
              <div class="setting-hint">{{ $t('settings.routineWakeHint') }}</div>
            </div>
          </div>
        </section>

        <!-- ====== 渠道 ====== -->
        <section v-show="activeTab === 'channels'">
          <h2 class="section-title">{{ $t('settings.channels') }}</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            {{ $t('settings.channelDesc1') }}
            <span class="font-mono">--settings</span> {{ $t('settings.channelDesc2') }}
          </p>

          <!-- 双列 chain -->
          <DragDropProvider @drag-end="onDragEnd">
          <div class="settings-grid">
            <!-- 会话链 -->
            <div>
              <div class="chain-title">{{ $t('settings.sessionChain') }}</div>
              <div class="chain-hint">{{ $t('settings.sessionChainHint') }}</div>
              <div class="chain-list">
                <SortableChainItem
                  v-for="(pid, idx) in sessionPrefixed"
                  :key="pid"
                  :id="pid"
                  :index="idx"
                  group="session"
                >
                  <div class="chain-content">
                    <div class="chain-row-1">
                      <span class="truncate font-medium text-xs">{{ channelById(stripPrefix(pid)).name }}</span>
                      <div class="chain-actions">
                        <button
                          :class="['form-toggle-sm', { on: channelById(stripPrefix(pid)).enabled }]"
                          @pointerdown.stop
                          @click="setChannelEnabled(stripPrefix(pid), !channelById(stripPrefix(pid)).enabled)"
                        >
                          <span class="form-toggle-knob" />
                        </button>
                        <template v-if="stripPrefix(pid) !== OFFICIAL_CHANNEL_ID">
                          <button class="chain-action" :title="$t('common.edit')" @pointerdown.stop @click="editing = channelById(stripPrefix(pid))"><span class="i-carbon-edit w-3 h-3" /></button>
                          <button class="chain-action text-destructive!" :title="$t('common.delete')" @pointerdown.stop @click="onDelete(channelById(stripPrefix(pid)))"><span class="i-carbon-trash-can w-3 h-3" /></button>
                        </template>
                      </div>
                    </div>
                    <div class="chain-row-2">
                      <template v-if="stripPrefix(pid) !== OFFICIAL_CHANNEL_ID">
                        <span v-if="channelById(stripPrefix(pid)).baseUrl" class="font-mono truncate">{{ channelById(stripPrefix(pid)).baseUrl }}</span>
                      </template>
                      <span v-else class="text-muted-foreground/60 italic">OAuth</span>
                      <span class="ml-auto shrink-0 flex items-center gap-1.5">
                        <template v-if="probing[stripPrefix(pid)]">
                          <span class="i-carbon-renew w-2.5 h-2.5 animate-spin" />
                        </template>
                        <template v-else-if="probeResults[stripPrefix(pid)]">
                          <span
                            class="inline-block w-1.5 h-1.5 rounded-full"
                            :class="probeResults[stripPrefix(pid)].online ? 'bg-green-600' : 'bg-destructive'"
                          />
                          <span v-if="probeResults[stripPrefix(pid)].online && probeResults[stripPrefix(pid)].models.length">
                            {{ probeResults[stripPrefix(pid)].models.length }} {{ $t('common.model').toLowerCase() }}s
                          </span>
                          <span v-else-if="!probeResults[stripPrefix(pid)].online">
                            {{ probeResults[stripPrefix(pid)].status === 'auth_error' ? '401' : probeResults[stripPrefix(pid)].status }}
                          </span>
                          <span v-if="probeResults[stripPrefix(pid)].latencyMs" class="text-muted-foreground/50">
                            {{ probeResults[stripPrefix(pid)].latencyMs }}ms
                          </span>
                        </template>
                        <button
                          v-if="stripPrefix(pid) !== OFFICIAL_CHANNEL_ID"
                          class="chain-action"
                          :title="$t('settings.probeChannel')"
                          @pointerdown.stop
                          @click="probeChannel(stripPrefix(pid))"
                        >
                          <span class="i-carbon-activity w-3 h-3" />
                        </button>
                      </span>
                    </div>
                  </div>
                </SortableChainItem>
              </div>
            </div>

            <!-- Agent 链 -->
            <div>
              <div class="chain-title">{{ $t('settings.agentChain') }}</div>
              <div class="chain-hint">{{ $t('settings.agentChainHint') }}</div>
              <div class="chain-list">
                <SortableChainItem
                  v-for="(pid, idx) in agentPrefixed"
                  :key="pid"
                  :id="pid"
                  :index="idx"
                  group="agent"
                >
                  <div class="chain-content">
                    <div class="chain-row-1">
                      <span class="truncate font-medium text-xs">{{ channelById(stripPrefix(pid)).name }}</span>
                      <div class="chain-actions">
                        <button
                          :class="['form-toggle-sm', { on: channelById(stripPrefix(pid)).enabled }]"
                          @pointerdown.stop
                          @click="setChannelEnabled(stripPrefix(pid), !channelById(stripPrefix(pid)).enabled)"
                        >
                          <span class="form-toggle-knob" />
                        </button>
                        <template v-if="stripPrefix(pid) !== OFFICIAL_CHANNEL_ID">
                          <button class="chain-action" :title="$t('common.edit')" @pointerdown.stop @click="editing = channelById(stripPrefix(pid))"><span class="i-carbon-edit w-3 h-3" /></button>
                          <button class="chain-action text-destructive!" :title="$t('common.delete')" @pointerdown.stop @click="onDelete(channelById(stripPrefix(pid)))"><span class="i-carbon-trash-can w-3 h-3" /></button>
                        </template>
                      </div>
                    </div>
                    <div class="chain-row-2">
                      <template v-if="stripPrefix(pid) !== OFFICIAL_CHANNEL_ID">
                        <span v-if="channelById(stripPrefix(pid)).baseUrl" class="font-mono truncate">{{ channelById(stripPrefix(pid)).baseUrl }}</span>
                      </template>
                      <span v-else class="text-muted-foreground/60 italic">OAuth</span>
                      <span class="ml-auto shrink-0 flex items-center gap-1.5">
                        <template v-if="probing[stripPrefix(pid)]">
                          <span class="i-carbon-renew w-2.5 h-2.5 animate-spin" />
                        </template>
                        <template v-else-if="probeResults[stripPrefix(pid)]">
                          <span
                            class="inline-block w-1.5 h-1.5 rounded-full"
                            :class="probeResults[stripPrefix(pid)].online ? 'bg-green-600' : 'bg-destructive'"
                          />
                          <span v-if="probeResults[stripPrefix(pid)].online && probeResults[stripPrefix(pid)].models.length">
                            {{ probeResults[stripPrefix(pid)].models.length }} {{ $t('common.model').toLowerCase() }}s
                          </span>
                          <span v-else-if="!probeResults[stripPrefix(pid)].online">
                            {{ probeResults[stripPrefix(pid)].status === 'auth_error' ? '401' : probeResults[stripPrefix(pid)].status }}
                          </span>
                          <span v-if="probeResults[stripPrefix(pid)].latencyMs" class="text-muted-foreground/50">
                            {{ probeResults[stripPrefix(pid)].latencyMs }}ms
                          </span>
                        </template>
                        <button
                          v-if="stripPrefix(pid) !== OFFICIAL_CHANNEL_ID"
                          class="chain-action"
                          :title="$t('settings.probeChannel')"
                          @pointerdown.stop
                          @click="probeChannel(stripPrefix(pid))"
                        >
                          <span class="i-carbon-activity w-3 h-3" />
                        </button>
                      </span>
                    </div>
                  </div>
                </SortableChainItem>
              </div>
            </div>
          </div>
          </DragDropProvider>

          <ChannelForm
            v-if="editing"
            :key="editing === 'new' ? '__new__' : editing.id"
            :channel="editing === 'new' ? null : editing"
            class="mt-3"
            @saved="onSaved"
            @cancel="editing = null"
          />

          <div class="flex items-center gap-2 mt-3">
            <button
              v-if="!editing"
              class="px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow"
              @click="editing = 'new'"
            >
              {{ $t('settings.addChannel') }}
            </button>
            <button
              class="px-2.5 py-1 text-xs rounded-md text-muted-foreground border border-border hover:text-foreground hover:bg-muted transition-colors"
              @click="onReveal"
            >
              {{ $t('common.openConfigDir') }}
            </button>
          </div>
        </section>

        <!-- ====== 模型 ====== -->
        <section v-show="activeTab === 'models'">
          <h2 class="section-title">{{ $t('settings.models') }}</h2>
          <div class="settings-grid">
            <!-- 顾问模式 -->
            <div class="sub-card">
              <h3 class="sub-card-title">{{ $t('settings.advisorMode') }}</h3>
              <div class="setting-cell mb-2">
                <div class="setting-label">{{ $t('settings.primaryModel') }}</div>
                <select v-model="advisorMain" class="form-select w-full">
                  <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
                  <option value="claude-haiku-4-5">claude-haiku-4-5</option>
                </select>
              </div>
              <div class="setting-cell">
                <div class="setting-label">{{ $t('settings.advisorModel') }}</div>
                <select v-model="advisorModel" class="form-select w-full">
                  <option value="claude-fable-5">claude-fable-5</option>
                  <option value="claude-opus-4-8">claude-opus-4-8</option>
                  <option value="claude-opus-4-6">claude-opus-4-6</option>
                </select>
              </div>
              <p class="text-[11px] text-accent mt-2">{{ $t('settings.advisorWarning') }}</p>
            </div>

            <!-- 模型可见性 -->
            <div class="sub-card">
              <h3 class="sub-card-title">{{ $t('settings.modelVisibility') }}</h3>
              <label class="form-checkbox-row">
                <input v-model="hideCreditsModels" type="checkbox" />
                <span>{{ $t('settings.hide1mModels') }}</span>
              </label>
              <label class="form-checkbox-row">
                <input v-model="autoDetectModels" type="checkbox" />
                <span>{{ $t('settings.autoDetect') }}</span>
              </label>
            </div>
          </div>
        </section>

        <!-- ====== Agent ====== -->
        <section v-show="activeTab === 'agent'">
          <h2 class="section-title">{{ $t('settings.agent') }}</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            {{ $t('settings.agentDesc') }}
          </p>
          <div class="settings-grid">
            <div v-for="a in agentKeys" :key="a.key" class="agent-item">
              <div class="flex-1 min-w-0">
                <div class="text-xs font-medium">{{ $t(a.label) }}</div>
                <div class="text-[11px] text-muted-foreground mt-0.5">{{ $t(a.desc) }}</div>
              </div>
              <button
                :class="['form-toggle', { on: isAgentEnabled(a.key) }]"
                @click="toggleAgent(a.key)"
              >
                <span class="form-toggle-knob" />
              </button>
            </div>
          </div>
        </section>

        <!-- ====== Claude Code 配置 ====== -->
        <section v-show="activeTab === 'claude-code'" class="cli-section">
          <ClaudeCodeSettings />
        </section>

        <!-- ====== 实验室 ====== -->
        <section v-show="activeTab === 'lab'">
          <h2 class="section-title">{{ $t('settings.lab') }}</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            {{ $t('settings.labDesc') }}
          </p>
          <div class="settings-grid mb-4">
            <div class="setting-cell">
              <label class="form-toggle">
                <input v-model="htmlVisualEnabled" type="checkbox">
                <span>{{ $t('settings.htmlVisual') }}</span>
              </label>
              <p class="form-hint">{{ $t('settings.htmlVisualDesc') }}</p>
            </div>
          </div>
          <div class="iframe-zone">
            <span class="iframe-badge">IFRAME</span>
            <AgentIframeDemo />
          </div>
        </section>

        <!-- ====== 诊断 ====== -->
        <section v-show="activeTab === 'diag'">
          <h2 class="section-title">{{ $t('settings.diagnostics') }}</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            {{ $t('settings.diagDesc') }}
          </p>
          <DiagnosisCard
            :diag="diag"
            :loading="diagLoading"
            :error="diagError"
            :scanned-at="diagAt"
            @retry="retryDiag"
          />
        </section>

      </div>
    </div>
    </div>
  </div>
</template>

<style scoped>
/* 侧栏 */
.side-nav {
  width: 140px;
  flex-shrink: 0;
  border-right: 1px solid var(--border);
  padding: 14px 8px;
  background: var(--background);
}
.side-title {
  font-size: 14px;
  font-weight: 600;
  padding: 0 8px;
  margin-bottom: 14px;
  display: flex;
  align-items: center;
  gap: 6px;
}
.side-item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 6px 10px;
  font-size: 12px;
  text-align: left;
  color: var(--muted-foreground);
  border-radius: var(--radius);
  transition: all 0.15s;
  margin-bottom: 2px;
  border: none;
  background: none;
  cursor: pointer;
}
.side-item:hover {
  color: var(--foreground);
  background: var(--muted);
}
.side-item.active {
  color: var(--primary);
  font-weight: 500;
  background: var(--card);
  box-shadow: var(--shadow-paper);
}

/* 内容体 */
.settings-body {
  padding: 20px;
}

.cli-section {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* 分区标题 */
.section-title {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 14px;
}

/* 双列网格 */
.settings-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}

/* 设置单元：label 在上，控件在下 */
.setting-cell {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.setting-label {
  font-size: 12px;
  font-weight: 500;
}
.setting-hint {
  font-size: 11px;
  color: var(--muted-foreground);
  font-weight: 400;
}

/* AI 翻译区 */
.translate-zone {
  padding: 8px 0 4px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.translate-form {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px;
  background: var(--card);
}

/* 子卡片 */
.sub-card {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px;
  margin-bottom: 12px;
  background: var(--card);
}
.sub-card-title {
  font-size: 12px;
  font-weight: 500;
  margin-bottom: 10px;
}

/* iframe 标识 */
.iframe-zone {
  border: 2px dashed var(--accent);
  border-radius: var(--radius);
  position: relative;
  overflow: hidden;
}
.iframe-badge {
  position: absolute;
  top: 0;
  right: 0;
  z-index: 2;
  padding: 2px 10px;
  font-size: 10px;
  font-weight: 600;
  background: var(--accent);
  color: var(--accent-foreground);
  border-radius: 0 0 0 var(--radius);
  letter-spacing: 0.04em;
}

/* 渠道链 */
.chain-title {
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 2px;
}
.chain-hint {
  font-size: 11px;
  color: var(--muted-foreground);
  margin-bottom: 6px;
}
.chain-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.chain-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.chain-row-1 {
  display: flex;
  align-items: center;
  gap: 6px;
}
.chain-row-2 {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--muted-foreground);
}
.chain-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  flex-shrink: 0;
}
.chain-action {
  padding: 2px;
  color: var(--muted-foreground);
  border-radius: 3px;
  transition: color 0.15s, background 0.15s;
  border: none;
  background: none;
  cursor: pointer;
  flex-shrink: 0;
}
.chain-action:hover {
  color: var(--foreground);
  background: var(--muted);
}
.agent-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--card);
}

/* 渠道标签 */
.channel-chip {
  padding: 0 4px;
  font-size: 9.5px;
  line-height: 16px;
  border: 1px solid var(--primary);
  color: var(--primary);
  border-radius: calc(var(--radius) - 2px);
  flex-shrink: 0;
}
</style>
