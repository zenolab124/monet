<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import {
  useChannels,
  refreshChannels,
  OFFICIAL_CHANNEL_ID,
  type ChannelInfo,
  type CcSwitchProvider,
} from '@/composables/useChannels'
import { useUiState } from '@/composables/useUiState'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import { useLocale } from '@/composables/useLocale'
import { useHomeStats } from '@/composables/useHomeStats'
import ChannelForm from '@/components/settings/ChannelForm.vue'
import OfficialDefaultsForm from '@/components/settings/OfficialDefaultsForm.vue'
import PaperSelect from '@/components/settings/PaperSelect.vue'
import DiagnosisCard from '@/components/home/DiagnosisCard.vue'
import AgentIframeDemo from '@/components/settings/AgentIframeDemo.vue'
import ClaudeCodeSettings from '@/components/settings/ClaudeCodeSettings.vue'
import PermissionsPanel from '@/components/settings/PermissionsPanel.vue'
import TurnSignalCard from '@/components/settings/TurnSignalCard.vue'
import { useWorkbench } from '@/composables/useWorkbench'
import { useZoom } from '@/composables/useZoom'
import { useHtmlVisual } from '@/features'
import { useTheme } from '@/composables/useTheme'
import { MODELS } from '@/utils/modelContext'

const { t } = useI18n()
const {
  channels, defaultSessionChannel, defaultAgentChannel, defaultAgentModel,
  probeResults, probing,
  revealedTokens, revealToken, hideToken, agentPreferences,
  deleteChannel, setChannelEnabled, setDefaultSessionChannel, setDefaultAgentModel,
  setAgentFeatureModel, revealChannelsDir,
  probeChannel, probeAllChannels, loadAgentPreferences,
  scanCcSwitch, importCcSwitch,
} = useChannels()
const { activeSection } = useUiState()
const { diag, diagLoading, diagError, diagAt, retryDiag, ensureLoaded } = useHomeStats()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()
const {
  locale, availableLocales, switchLocale,
  translating, translateError, parseLanguageIntent, translateLocale, deleteLocale, isBuiltin,
} = useLocale()

const { minColumnWidth, setMinColumnWidth } = useWorkbench()
const { zoomLevel, setZoom, MIN_ZOOM, MAX_ZOOM, STEP } = useZoom()
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

// --- Agent 渠道测试 ---
interface AgentTestResult {
  success: boolean
  channelId: string
  model: string
  durationMs: number
  inputTokens: number
  outputTokens: number
  reply: string
  error?: string
}
const agentTesting = ref(false)
const agentTestResult = ref<AgentTestResult | null>(null)

async function onTestAgent() {
  agentTesting.value = true
  agentTestResult.value = null
  try {
    agentTestResult.value = await invoke<AgentTestResult>('test_agent_channel')
  } catch (e) {
    agentTestResult.value = { success: false, channelId: '', model: '', durationMs: 0, inputTokens: 0, outputTokens: 0, reply: '', error: String(e) }
  } finally {
    agentTesting.value = false
  }
}

// --- Agent 调用日志 ---
interface AgentLogEntry {
  timestamp: string
  feature: string
  channelId: string
  model: string
  durationMs: number
  inputTokens: number
  outputTokens: number
  success: boolean
  error?: string
}
const agentLogs = ref<AgentLogEntry[]>([])
const agentLogsLoading = ref(false)
const showAgentLogs = ref(false)

async function loadAgentLogs() {
  agentLogsLoading.value = true
  try {
    agentLogs.value = await invoke<AgentLogEntry[]>('get_agent_logs')
  } finally {
    agentLogsLoading.value = false
  }
}

async function clearAgentLogs() {
  const ok = await confirm(t('settings.agentLogsClearConfirm'))
  if (!ok) return
  await invoke('clear_agent_logs')
  agentLogs.value = []
}

const agentLogsSorted = computed(() => [...agentLogs.value].reverse())

const agentLogsStats = computed(() => {
  const logs = agentLogs.value
  const totalInput = logs.reduce((s, l) => s + l.inputTokens, 0)
  const totalOutput = logs.reduce((s, l) => s + l.outputTokens, 0)
  const successCount = logs.filter(l => l.success).length
  return { total: logs.length, totalInput, totalOutput, successCount }
})

type Tab = 'general' | 'channels' | 'models' | 'agent' | 'claude-code' | 'permissions' | 'extensions' | 'lab' | 'diag'
const activeTab = ref<Tab>('general')

const editing = ref<'new' | ChannelInfo | null>(null)
/** official 渠道轻量编辑(仅默认模型/思考强度两字段) */
const editingOfficial = ref<ChannelInfo | null>(null)

const ccSwitchProviders = ref<CcSwitchProvider[]>([])
const ccSwitchSelected = ref<Set<string>>(new Set())
const ccSwitchScanning = ref(false)
const ccSwitchOpen = ref(false)

async function onScanCcSwitch() {
  ccSwitchScanning.value = true
  try {
    const list = await scanCcSwitch()
    ccSwitchProviders.value = list
    ccSwitchSelected.value = new Set(list.filter(p => !p.alreadyImported).map(p => p.id))
    ccSwitchOpen.value = true
  } catch {
    notifyTransient(t('settings.ccSwitchNotFound'))
  } finally {
    ccSwitchScanning.value = false
  }
}

async function onImportCcSwitch() {
  const ids = [...ccSwitchSelected.value]
  if (!ids.length) return
  const count = await importCcSwitch(ids)
  notifyTransient(t('settings.ccSwitchImported', { count }))
  ccSwitchOpen.value = false
  ccSwitchProviders.value = []
}

function toggleCcSwitchAll() {
  const importable = ccSwitchProviders.value.filter(p => !p.alreadyImported)
  if (ccSwitchSelected.value.size === importable.length) {
    ccSwitchSelected.value = new Set()
  } else {
    ccSwitchSelected.value = new Set(importable.map(p => p.id))
  }
}

const rcEnabled = ref(true)
const wakePolicy = ref('passive')
const widgetDayStart = ref(0)

async function loadWidgetConfig() {
  try {
    const cfg = await invoke<{ dayStartHour: number }>('get_widget_config')
    widgetDayStart.value = cfg.dayStartHour
  } catch {}
}
async function setWidgetDayStart(hour: number) {
  widgetDayStart.value = hour
  await invoke('set_widget_config', { dayStartHour: hour }).catch(() => {})
}

// 系统授权状态：/etc/sudoers.d 白名单是否在位（与 policy 独立——
// 切回被动后规则可保留，下次开启不再弹密码框）
const wakeAuthorized = ref(false)

async function loadWakePolicy() {
  try {
    wakePolicy.value = await invoke<string>('get_routine_wake_policy')
    wakeAuthorized.value = await invoke<boolean>('get_wake_authorization_status')
  } catch {}
}
async function setWakePolicy(policy: string) {
  if (policy === 'active') {
    // 乐观跟随 radio，取消/失败回弹（值变化驱动 DOM 复位）
    wakePolicy.value = 'active'
    const ok = await confirm(
      t('settings.routineWakeAuthBody'),
      t('settings.routineWakeAuthConfirm'),
    )
    if (ok) {
      try {
        await invoke('enable_wake_active')
        wakeAuthorized.value = true
        return
      } catch (e) {
        const msg = String(e)
        notifyTransient(
          t('settings.routineWake'),
          msg.includes('cancelled') ? t('settings.routineWakeAuthDenied') : msg,
        )
      }
    }
    wakePolicy.value = 'passive'
    return
  }
  wakePolicy.value = 'passive'
  try {
    await invoke('set_routine_wake_policy', { policy: 'passive' })
  } catch {}
}

async function removeWakeAuthorization() {
  try {
    await invoke('remove_wake_authorization')
    notifyTransient(t('settings.routineWake'), t('settings.routineWakeAuthRemoved'))
  } catch (e) {
    if (!String(e).includes('cancelled')) {
      notifyTransient(t('settings.routineWake'), String(e))
    }
  }
  // 提权删除可能被取消（规则仍在、策略已降级），以后端真实状态为准
  await loadWakePolicy()
}

// 模型 — 暂用本地状态
const advisorMain = ref('claude-sonnet-4-6')
const advisorModel = ref('claude-fable-5')
const hideCreditsModels = ref(false)
const autoDetectModels = ref(false)

// 顾问主/顾问模型下拉:从 MODELS 非 legacy 项派生(单源,消灭硬编码 <option>)。
// 行为兼容:若当前值不在派生列表(如默认 claude-sonnet-4-6 为 legacy 项),附加显示。
const nonLegacyModels = computed(() =>
  MODELS.filter(m => !m.legacy).map(m => ({ value: m.id, label: m.label })),
)
function withCurrent(list: { value: string; label: string }[], current: string) {
  if (current && !list.some(o => o.value === current)) {
    return [...list, { value: current, label: current }]
  }
  return list
}
const advisorMainOptions = computed(() => withCurrent(nonLegacyModels.value, advisorMain.value))
const advisorModelOptions = computed(() => withCurrent(nonLegacyModels.value, advisorModel.value))

onMounted(() => { refreshChannels(); loadAgentToggles(); loadAgentPreferences(); loadWakePolicy(); loadWidgetConfig() })

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

const sessionChannels = () => channels.value.filter(c => c.scope !== 'agent-only')
const agentOnlyChannels = () => channels.value.filter(c => c.scope === 'agent-only')

// 官方渠道 Agent 可选模型:从 modelContext 的 MODELS 派生(单源,消灭第二份清单)。
// 取具体版本 id 并剥 [1m] 后缀(Agent 用的是 API 模型名,1M 由 CLI/请求侧处理),去重。
const OFFICIAL_MODELS = [
  ...new Set(MODELS.map(m => m.id.replace(/\[1m\]$/i, ''))),
]

const agentChannelId = ref(defaultAgentChannel.value ?? OFFICIAL_CHANNEL_ID)
watch(defaultAgentChannel, (v) => { agentChannelId.value = v ?? OFFICIAL_CHANNEL_ID })

const agentModelsForChannel = computed(() => {
  if (agentChannelId.value === OFFICIAL_CHANNEL_ID) return OFFICIAL_MODELS
  const ch = channels.value.find(c => c.id === agentChannelId.value)
  const probeModels = probeResults.value[agentChannelId.value]?.models ?? []
  const saved = ch ? [...ch.availableModels, ...(ch.agentModel ? [ch.agentModel] : [])] : []
  const all = [...new Set([...probeModels, ...saved])]
  return all.length ? all.sort() : []
})

const agentChannelSelectOptions = computed(() => {
  const opts = [{ value: OFFICIAL_CHANNEL_ID, label: 'Official' }]
  for (const ch of channels.value) {
    if (ch.id !== OFFICIAL_CHANNEL_ID && ch.enabled) {
      opts.push({ value: ch.id, label: ch.name })
    }
  }
  return opts
})

const agentModelSelectOptions = computed(() =>
  agentModelsForChannel.value.map(m => ({ value: m, label: m }))
)

function onAgentChannelChange(id: string) {
  agentChannelId.value = id
  const models = id === OFFICIAL_CHANNEL_ID ? OFFICIAL_MODELS : agentModelsForChannel.value
  setDefaultAgentModel(
    id === OFFICIAL_CHANNEL_ID ? null : id,
    models[0] ?? null,
  )
}

function onAgentModelChange(model: string) {
  setDefaultAgentModel(
    agentChannelId.value === OFFICIAL_CHANNEL_ID ? null : agentChannelId.value,
    model || null,
  )
}

const agentModelOptions = () => {
  const opts: { channel: string; channelName: string; model: string }[] = []
  for (const m of OFFICIAL_MODELS) {
    opts.push({ channel: OFFICIAL_CHANNEL_ID, channelName: 'Official', model: m })
  }
  for (const ch of channels.value) {
    if (!ch.enabled || ch.id === OFFICIAL_CHANNEL_ID) continue
    const models = new Set([...ch.availableModels, ...(ch.agentModel ? [ch.agentModel] : [])])
    for (const m of models) {
      opts.push({ channel: ch.id, channelName: ch.name, model: m })
    }
  }
  return opts
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
      <button :class="['side-item', { active: activeTab === 'permissions' }]" @click="activeTab = 'permissions'">
        <span class="i-carbon-security w-3.5 h-3.5" />{{ $t('settings.permissionsNav') }}
      </button>
      <button :class="['side-item', { active: activeTab === 'extensions' }]" @click="activeTab = 'extensions'">
        <span class="i-carbon-plug w-3.5 h-3.5" />{{ $t('settings.extensions') }}
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
                      v-tooltip="$t('common.delete')"
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
                <div v-if="wakeAuthorized" class="flex items-center gap-2 ml-5 text-[11px] text-muted-foreground">
                  <span>{{ $t('settings.routineWakeAuthorized') }}</span>
                  <button
                    class="underline underline-offset-2 hover:text-foreground transition-colors"
                    @click="removeWakeAuthorization"
                  >
                    {{ $t('settings.routineWakeRemoveAuth') }}
                  </button>
                </div>
              </div>
              <div class="setting-hint">{{ $t('settings.routineWakeHint') }}</div>
            </div>
            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.zoomLevel') }}</div>
              <div class="flex items-center gap-2.5">
                <input
                  type="range"
                  :value="zoomLevel"
                  :min="MIN_ZOOM"
                  :max="MAX_ZOOM"
                  :step="STEP"
                  class="flex-1 accent-primary"
                  @input="setZoom(Number(($event.target as HTMLInputElement).value))"
                />
                <span class="text-xs tabular-nums text-muted-foreground w-9 text-right">{{ Math.round(zoomLevel * 100) }}%</span>
              </div>
              <div class="setting-hint">{{ $t('settings.zoomLevelHint') }}</div>
            </div>

            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.minColumnWidth') }}</div>
              <div class="flex items-center gap-2">
                <input
                  type="number"
                  :value="minColumnWidth"
                  min="200"
                  step="10"
                  class="form-input w-24 tabular-nums"
                  @change="setMinColumnWidth(Number(($event.target as HTMLInputElement).value))"
                />
                <span class="text-[11px] text-muted-foreground">px</span>
              </div>
              <div class="setting-hint">{{ $t('settings.minColumnWidthHint') }}</div>
            </div>

            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.widgetDayBoundary') }}</div>
              <select
                :value="widgetDayStart"
                class="form-select w-full"
                @change="setWidgetDayStart(Number(($event.target as HTMLSelectElement).value))"
              >
                <option :value="0">{{ $t('settings.widgetMidnight') }}</option>
                <option :value="5">{{ $t('settings.widgetFiveAm') }}</option>
                <option :value="-1">{{ $t('settings.widgetRolling24h') }}</option>
              </select>
              <div class="setting-hint">{{ $t('settings.widgetDayBoundaryHint') }}</div>
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

          <!-- 双列渠道 -->
          <div class="settings-grid">
            <!-- 左列：会话渠道 -->
            <div>
              <div class="chain-title">{{ $t('settings.defaultSessionChannel') }}</div>
              <div class="chain-list">
                <div
                  v-for="ch in sessionChannels()"
                  :key="ch.id"
                  class="chain-item"
                  :class="{ 'chain-item-active': (defaultSessionChannel ?? OFFICIAL_CHANNEL_ID) === ch.id }"
                  @click="setDefaultSessionChannel(ch.id === OFFICIAL_CHANNEL_ID ? null : ch.id)"
                >
                  <div class="chain-content">
                    <div class="chain-row-1">
                      <span class="truncate font-medium text-xs">{{ ch.name }}</span>
                      <div class="chain-actions">
                        <button :class="['form-toggle-sm', { on: ch.enabled }]" @click.stop="setChannelEnabled(ch.id, !ch.enabled)"><span class="form-toggle-knob" /></button>
                        <template v-if="ch.id !== OFFICIAL_CHANNEL_ID">
                          <button class="icon-btn icon-btn-sm icon-btn-ghost" v-tooltip="$t('common.edit')" @click.stop="editing = ch"><span class="i-carbon-edit w-3 h-3" /></button>
                          <button class="icon-btn icon-btn-sm icon-btn-ghost icon-btn-danger" v-tooltip="$t('common.delete')" @click.stop="onDelete(ch)"><span class="i-carbon-trash-can w-3 h-3" /></button>
                        </template>
                        <button v-else class="icon-btn icon-btn-sm icon-btn-ghost" v-tooltip="$t('settings.officialDefaults.edit')" @click.stop="editingOfficial = ch"><span class="i-carbon-edit w-3 h-3" /></button>
                      </div>
                    </div>
                    <div class="chain-row-2">
                      <template v-if="ch.id !== OFFICIAL_CHANNEL_ID">
                        <span v-if="ch.baseUrl" class="font-mono truncate">{{ ch.baseUrl }}</span>
                      </template>
                      <span v-else class="text-muted-foreground/60 italic">OAuth</span>
                      <span v-if="ch.defaultModel" class="text-[10px] font-mono text-accent-foreground/60 bg-accent/40 px-1 rounded shrink-0" v-tooltip="$t('settings.channelForm.defaultModelLabel')">{{ ch.defaultModel }}</span>
                      <span v-if="ch.defaultEffort" class="text-[10px] font-mono text-accent-foreground/60 bg-accent/40 px-1 rounded shrink-0" v-tooltip="$t('settings.channelForm.defaultEffortLabel')">{{ ch.defaultEffort }}</span>
                      <span v-if="ch.agentModel" class="text-[10px] font-mono text-accent-foreground/60 bg-accent/40 px-1 rounded shrink-0">{{ ch.agentModel }}</span>
                      <span class="ml-auto shrink-0 flex items-center gap-1.5">
                        <template v-if="probing[ch.id]"><span class="i-carbon-renew w-2.5 h-2.5 animate-spin" /></template>
                        <template v-else-if="probeResults[ch.id]">
                          <span class="inline-block w-1.5 h-1.5 rounded-full" :class="probeResults[ch.id].online ? 'bg-green-600' : 'bg-destructive'" />
                          <span v-if="probeResults[ch.id].online && probeResults[ch.id].models.length" v-tooltip="probeResults[ch.id].models.join('\n')">{{ probeResults[ch.id].models.length }} models</span>
                          <span v-else-if="!probeResults[ch.id].online">{{ probeResults[ch.id].status === 'auth_error' ? '401' : probeResults[ch.id].status }}</span>
                          <span v-if="probeResults[ch.id].latencyMs" class="text-muted-foreground/50">{{ probeResults[ch.id].latencyMs }}ms</span>
                        </template>
                        <button v-if="ch.id !== OFFICIAL_CHANNEL_ID" class="icon-btn icon-btn-sm icon-btn-ghost" v-tooltip="$t('settings.probeChannel')" @click.stop="probeChannel(ch.id)"><span class="i-carbon-activity w-3 h-3" /></button>
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- 右列：Agent 渠道 -->
            <div>
              <div class="chain-title">{{ $t('settings.defaultAgentModel') }}</div>
              <!-- 级联下拉 -->
              <div class="flex gap-1.5 mb-2">
                <PaperSelect
                  :options="agentChannelSelectOptions"
                  :model-value="agentChannelId"
                  @update:model-value="onAgentChannelChange"
                />
                <PaperSelect
                  :options="agentModelSelectOptions"
                  :model-value="defaultAgentModel ?? ''"
                  mono
                  editable
                  placeholder="model name"
                  @update:model-value="onAgentModelChange"
                />
              </div>
              <!-- Agent-only 渠道列表 -->
              <div class="chain-list">
                <div v-for="ch in agentOnlyChannels()" :key="ch.id" class="chain-item">
                  <div class="chain-content">
                    <div class="chain-row-1">
                      <span class="truncate font-medium text-xs">{{ ch.name }}</span>
                      <div class="chain-actions">
                        <button :class="['form-toggle-sm', { on: ch.enabled }]" @click="setChannelEnabled(ch.id, !ch.enabled)"><span class="form-toggle-knob" /></button>
                        <button class="icon-btn icon-btn-sm icon-btn-ghost" v-tooltip="$t('common.edit')" @click="editing = ch"><span class="i-carbon-edit w-3 h-3" /></button>
                        <button class="icon-btn icon-btn-sm icon-btn-ghost icon-btn-danger" v-tooltip="$t('common.delete')" @click="onDelete(ch)"><span class="i-carbon-trash-can w-3 h-3" /></button>
                      </div>
                    </div>
                    <div class="chain-row-2">
                      <span v-if="ch.baseUrl" class="font-mono truncate">{{ ch.baseUrl }}</span>
                      <span v-if="ch.agentModel" class="text-[10px] font-mono text-accent-foreground/60 bg-accent/40 px-1 rounded shrink-0">{{ ch.agentModel }}</span>
                      <span class="ml-auto shrink-0 flex items-center gap-1.5">
                        <template v-if="probing[ch.id]"><span class="i-carbon-renew w-2.5 h-2.5 animate-spin" /></template>
                        <template v-else-if="probeResults[ch.id]">
                          <span class="inline-block w-1.5 h-1.5 rounded-full" :class="probeResults[ch.id].online ? 'bg-green-600' : 'bg-destructive'" />
                          <span v-if="probeResults[ch.id].online && probeResults[ch.id].models.length" v-tooltip="probeResults[ch.id].models.join('\n')">{{ probeResults[ch.id].models.length }} models</span>
                          <span v-else-if="!probeResults[ch.id].online">{{ probeResults[ch.id].status === 'auth_error' ? '401' : probeResults[ch.id].status }}</span>
                          <span v-if="probeResults[ch.id].latencyMs" class="text-muted-foreground/50">{{ probeResults[ch.id].latencyMs }}ms</span>
                        </template>
                        <button class="icon-btn icon-btn-sm icon-btn-ghost" v-tooltip="$t('settings.probeChannel')" @click="probeChannel(ch.id)"><span class="i-carbon-activity w-3 h-3" /></button>
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <ChannelForm
            v-if="editing"
            :key="editing === 'new' ? '__new__' : editing.id"
            :channel="editing === 'new' ? null : editing"
            class="mt-3"
            @saved="onSaved"
            @cancel="editing = null"
          />

          <OfficialDefaultsForm
            v-if="editingOfficial"
            :channel="editingOfficial"
            class="mt-3"
            @saved="editingOfficial = null"
            @cancel="editingOfficial = null"
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
              :disabled="ccSwitchScanning"
              @click="onScanCcSwitch"
            >
              <span v-if="ccSwitchScanning" class="i-carbon-renew w-3 h-3 animate-spin mr-1 inline-block align-[-2px]" />
              {{ ccSwitchScanning ? $t('settings.ccSwitchScanning') : $t('settings.ccSwitchImport') }}
            </button>
            <button
              class="px-2.5 py-1 text-xs rounded-md text-muted-foreground border border-border hover:text-foreground hover:bg-muted transition-colors"
              @click="onReveal"
            >
              {{ $t('common.openConfigDir') }}
            </button>
          </div>

          <!-- CC Switch 导入列表 -->
          <div v-if="ccSwitchOpen && ccSwitchProviders.length" class="mt-3 rounded-md border border-border bg-popover p-3">
            <div class="flex items-center justify-between mb-2">
              <span class="text-xs font-medium">CC Switch ({{ ccSwitchProviders.length }})</span>
              <div class="flex items-center gap-2">
                <button class="text-[10px] text-muted-foreground hover:text-foreground" @click="toggleCcSwitchAll">{{ $t('settings.ccSwitchSelectAll') }}</button>
                <button
                  class="px-2 py-0.5 text-[11px] rounded bg-primary text-primary-foreground disabled:opacity-40"
                  :disabled="ccSwitchSelected.size === 0"
                  @click="onImportCcSwitch"
                >
                  {{ $t('settings.ccSwitchImportSelected') }} ({{ ccSwitchSelected.size }})
                </button>
                <button class="icon-btn icon-btn-sm icon-btn-ghost" @click="ccSwitchOpen = false"><span class="i-carbon-close w-3 h-3" /></button>
              </div>
            </div>
            <div class="flex flex-col gap-1 max-h-48 overflow-y-auto">
              <label
                v-for="p in ccSwitchProviders"
                :key="p.id"
                class="flex items-center gap-2 px-2 py-1 rounded text-xs hover:bg-muted/50 cursor-pointer"
                :class="{ 'opacity-50': p.alreadyImported }"
              >
                <input
                  type="checkbox"
                  :checked="ccSwitchSelected.has(p.id)"
                  :disabled="p.alreadyImported"
                  class="accent-primary"
                  @change="p.alreadyImported ? null : (ccSwitchSelected.has(p.id) ? ccSwitchSelected.delete(p.id) : ccSwitchSelected.add(p.id))"
                />
                <span class="font-medium truncate">{{ p.name }}</span>
                <span v-if="p.isCurrent" class="text-[10px] text-green-600 shrink-0">{{ $t('settings.ccSwitchCurrent') }}</span>
                <span v-if="p.alreadyImported" class="text-[10px] text-muted-foreground shrink-0">{{ $t('settings.ccSwitchAlready') }}</span>
                <span v-if="p.baseUrl" class="ml-auto text-[10px] font-mono text-muted-foreground truncate max-w-48">{{ p.baseUrl }}</span>
              </label>
            </div>
          </div>
          <p v-else-if="ccSwitchOpen && !ccSwitchProviders.length && !ccSwitchScanning" class="mt-2 text-xs text-muted-foreground">{{ $t('settings.ccSwitchEmpty') }}</p>
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
                  <option v-for="o in advisorMainOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
                </select>
              </div>
              <div class="setting-cell">
                <div class="setting-label">{{ $t('settings.advisorModel') }}</div>
                <select v-model="advisorModel" class="form-select w-full">
                  <option v-for="o in advisorModelOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
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
                <select
                  v-if="isAgentEnabled(a.key)"
                  class="form-input text-[11px] font-mono mt-1.5 w-auto max-w-56 h-6 py-0"
                  :value="agentPreferences[a.key]?.preferredChannel && agentPreferences[a.key]?.preferredModel ? `${agentPreferences[a.key].preferredChannel}:${agentPreferences[a.key].preferredModel}` : ''"
                  @change="{ const v = ($event.target as HTMLSelectElement).value; if (!v) { setAgentFeatureModel(a.key, null, null) } else { const [ch, ...rest] = v.split(':'); setAgentFeatureModel(a.key, ch, rest.join(':')) } }"
                >
                  <option value="">{{ $t('settings.agentAutoChannel') }}</option>
                  <option v-for="opt in agentModelOptions()" :key="`${opt.channel}:${opt.model}`" :value="`${opt.channel}:${opt.model}`">
                    {{ opt.channelName }} / {{ opt.model }}
                  </option>
                </select>
              </div>
              <button
                :class="['form-toggle', { on: isAgentEnabled(a.key) }]"
                @click="toggleAgent(a.key)"
              >
                <span class="form-toggle-knob" />
              </button>
            </div>
          </div>

          <!-- Agent 操作栏 -->
          <div class="mt-6 pt-4 border-t border-border flex items-center gap-4">
            <button
              class="px-2.5 py-1 text-xs rounded-md border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
              :disabled="agentTesting"
              @click="onTestAgent"
            >
              <span v-if="agentTesting" class="i-carbon-renew w-3 h-3 animate-spin mr-1 inline-block align-[-2px]" />
              {{ $t('settings.agentTest') }}
            </button>
            <button
              class="text-xs text-primary hover:underline"
              @click="showAgentLogs = true; loadAgentLogs()"
            >{{ $t('settings.agentLogs') }}</button>
          </div>
          <!-- 测试结果 -->
          <div v-if="agentTestResult" class="mt-2 px-3 py-2 rounded-md border text-[11px]"
            :class="agentTestResult.success ? 'border-emerald-500/30 bg-emerald-500/5' : 'border-destructive/30 bg-destructive/5'"
          >
            <div v-if="agentTestResult.success" class="flex items-center gap-3 text-foreground">
              <span class="text-emerald-600 dark:text-emerald-400 font-medium">OK</span>
              <span class="text-muted-foreground">{{ agentTestResult.channelId }}</span>
              <span class="font-mono text-muted-foreground">{{ agentTestResult.model }}</span>
              <span class="font-mono text-muted-foreground">{{ agentTestResult.durationMs >= 1000 ? `${(agentTestResult.durationMs / 1000).toFixed(1)}s` : `${agentTestResult.durationMs}ms` }}</span>
              <span v-if="agentTestResult.inputTokens" class="font-mono text-muted-foreground">↑{{ agentTestResult.inputTokens }} ↓{{ agentTestResult.outputTokens }}</span>
            </div>
            <div v-else class="text-destructive">{{ agentTestResult.error }}</div>
          </div>
        </section>

        <!-- ====== Claude Code 配置 ====== -->
        <section v-show="activeTab === 'claude-code'" class="cli-section">
          <ClaudeCodeSettings />
        </section>

        <!-- ====== 权限体检 ====== -->
        <section v-show="activeTab === 'permissions'">
          <h2 class="section-title">{{ $t('settings.permCheck.title') }}</h2>
          <PermissionsPanel />
        </section>

        <!-- ====== 扩展 ====== -->
        <section v-show="activeTab === 'extensions'">
          <h2 class="section-title">{{ $t('settings.extensions') }}</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            {{ $t('settings.extensionsDesc') }}
          </p>
          <TurnSignalCard />
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

    <!-- Agent 日志弹窗 -->
  <div
    v-if="showAgentLogs"
    class="fixed inset-0 z-70 grid place-items-center"
    style="background: rgba(70, 45, 20, 0.18)"
    @mousedown.self="showAgentLogs = false"
  >
    <div class="w-[720px] max-w-[90vw] max-h-[80vh] rounded-lg bg-popover border border-border shadow-paper-lifted flex flex-col">
      <div class="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <h3 class="text-sm font-medium">{{ $t('settings.agentLogs') }}</h3>
        <div class="flex items-center gap-3">
          <button
            v-if="agentLogs.length"
            class="text-[11px] text-muted-foreground hover:text-destructive transition-colors"
            @click="clearAgentLogs"
          >{{ $t('common.clear') }}</button>
          <button
            class="text-[11px] text-muted-foreground hover:text-foreground transition-colors"
            @click="loadAgentLogs"
          >↻</button>
          <button
            class="i-carbon-close w-4 h-4 text-muted-foreground hover:text-foreground transition-colors"
            @click="showAgentLogs = false"
          />
        </div>
      </div>

      <div class="flex-1 overflow-auto">
        <div v-if="agentLogsLoading" class="text-xs text-muted-foreground py-8 text-center">
          {{ $t('common.loading') }}
        </div>
        <template v-else-if="agentLogs.length">
          <div class="flex gap-4 px-4 py-2 text-[11px] text-muted-foreground border-b border-border bg-muted/30">
            <span>{{ $t('settings.agentLogsTotal', { n: agentLogsStats.total }) }}</span>
            <span>{{ $t('settings.agentLogsSuccess', { n: agentLogsStats.successCount }) }}</span>
            <span>↑{{ agentLogsStats.totalInput.toLocaleString() }} ↓{{ agentLogsStats.totalOutput.toLocaleString() }} tokens</span>
          </div>
          <table class="agent-logs-table">
            <thead>
              <tr>
                <th>{{ $t('settings.agentLogsTime') }}</th>
                <th>{{ $t('settings.agentLogsFeature') }}</th>
                <th>{{ $t('settings.agentLogsChannel') }}</th>
                <th>{{ $t('settings.agentLogsModel') }}</th>
                <th class="text-right">{{ $t('settings.agentLogsDuration') }}</th>
                <th class="text-right">Tokens</th>
                <th>{{ $t('settings.agentLogsStatus') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(log, i) in agentLogsSorted" :key="i" :class="{ 'opacity-60': !log.success }">
                <td class="font-mono whitespace-nowrap">{{ new Date(log.timestamp).toLocaleString() }}</td>
                <td>{{ $t(`settings.agentFeature_${log.feature}`, log.feature) }}</td>
                <td class="font-mono">{{ log.channelId || 'official' }}</td>
                <td class="font-mono truncate max-w-32" :title="log.model">{{ log.model }}</td>
                <td class="text-right font-mono">{{ log.durationMs >= 1000 ? `${(log.durationMs / 1000).toFixed(1)}s` : `${log.durationMs}ms` }}</td>
                <td class="text-right font-mono">
                  <template v-if="log.inputTokens || log.outputTokens">↑{{ log.inputTokens }} ↓{{ log.outputTokens }}</template>
                  <span v-else class="text-muted-foreground">—</span>
                </td>
                <td>
                  <span v-if="log.success" class="text-emerald-600 dark:text-emerald-400">OK</span>
                  <span v-else class="text-destructive cursor-help" :title="log.error">FAIL</span>
                </td>
              </tr>
            </tbody>
          </table>
        </template>
        <div v-else class="text-xs text-muted-foreground py-8 text-center">
          {{ $t('settings.agentLogsEmpty') }}
        </div>
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
.chain-item {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 5px 8px;
  border-radius: var(--radius);
  border: 1px solid transparent;
  cursor: pointer;
  transition: all 0.15s;
}
.chain-item:hover {
  background: var(--muted);
}
.chain-item-active {
  border-color: var(--primary);
  background: color-mix(in srgb, var(--primary) 6%, transparent);
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

/* Agent 日志表格 */
.agent-logs-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 11px;
}
.agent-logs-table th {
  position: sticky;
  top: 0;
  background: var(--muted);
  padding: 4px 8px;
  text-align: left;
  font-weight: 500;
  color: var(--muted-foreground);
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}
.agent-logs-table td {
  padding: 3px 8px;
  border-bottom: 1px solid var(--border);
  color: var(--foreground);
}
.agent-logs-table tbody tr:hover {
  background: var(--muted);
}
</style>
