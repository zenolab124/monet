<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
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

const { t } = useI18n()
const { channels, defaultChannelId, deleteChannel, setDefaultChannel, revealChannelsDir } =
  useChannels()
const { activeSection } = useUiState()
const { diag, diagLoading, diagError, diagAt, retryDiag, ensureLoaded } = useHomeStats()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()
const {
  locale, availableLocales, switchLocale,
  translating, translateError, parseLanguageIntent, translateLocale, deleteLocale, isBuiltin,
} = useLocale()

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

type Tab = 'general' | 'channels' | 'models' | 'claude-code' | 'lab' | 'diag'
const activeTab = ref<Tab>('general')

const editing = ref<'new' | ChannelInfo | null>(null)

// 常规 — 暂用本地状态，后续接持久化
const defaultEffort = ref('cli')
const rcEnabled = ref(true)

// 模型 — 暂用本地状态
const advisorMain = ref('claude-sonnet-4-6')
const advisorModel = ref('claude-fable-5')
const hideCreditsModels = ref(false)
const autoDetectModels = ref(false)

onMounted(() => refreshChannels())

watch(activeSection, (s) => {
  if (s === 'settings') {
    refreshChannels()
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

async function onDefaultChange(e: Event) {
  const value = (e.target as HTMLSelectElement).value
  try {
    await setDefaultChannel(value === OFFICIAL_CHANNEL_ID ? null : value)
  } catch (err) {
    notifyTransient(t('settings.setDefaultFailed'), String(err))
    await refreshChannels()
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
    <div class="flex-1 min-w-0 overflow-y-auto">
      <div class="settings-body">

        <!-- ====== 常规 ====== -->
        <section v-show="activeTab === 'general'">
          <h2 class="section-title">{{ $t('settings.general') }}</h2>
          <div class="settings-grid">
            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.language') }}</div>
              <select
                :value="locale"
                class="ctrl-select w-full"
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
                      class="ctrl-input flex-1"
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
              <select v-model="defaultEffort" class="ctrl-select w-full">
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
                  :class="['toggle-track', { on: rcEnabled }]"
                  @click="rcEnabled = !rcEnabled"
                >
                  <span class="toggle-knob" />
                </button>
                <span class="text-[11px] text-muted-foreground">{{ $t('settings.remoteControlSub') }}</span>
              </div>
              <div class="setting-hint">{{ $t('settings.remoteControlHint') }}</div>
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

          <div class="settings-grid">
            <div class="setting-cell">
              <div class="setting-label">{{ $t('settings.defaultChannel') }}</div>
              <select
                :value="defaultChannelId ?? OFFICIAL_CHANNEL_ID"
                class="ctrl-select w-full"
                @change="onDefaultChange"
              >
                <option :value="OFFICIAL_CHANNEL_ID">{{ $t('settings.officialChannel') }}</option>
                <option v-for="c in channels" :key="c.id" :value="c.id">{{ c.name }}</option>
              </select>
            </div>
          </div>

          <!-- 渠道列表 -->
          <div class="settings-grid mt-3">
            <div
              v-for="c in channels"
              :key="c.id"
              class="rounded-md border border-border bg-card px-3 py-2 flex items-center gap-3"
            >
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5 text-xs">
                  <span class="font-medium truncate">{{ c.name }}</span>
                  <span class="text-muted-foreground font-mono">{{ c.id }}</span>
                  <span v-if="c.isDefault" class="channel-chip">{{ $t('settings.defaultBadge') }}</span>
                  <span v-if="!c.valid" class="channel-chip text-destructive border-destructive">{{ $t('settings.jsonParseFailed') }}</span>
                </div>
                <div class="text-[11px] text-muted-foreground truncate mt-0.5 font-mono">
                  {{ c.baseUrl ?? $t('settings.noBaseUrl') }}
                  <span v-if="c.authTokenMasked" class="ml-1.5">{{ $t('settings.tokenPrefix') }}{{ c.authTokenMasked }}</span>
                </div>
                <div v-if="c.extraEnvKeys.length || c.note" class="text-[11px] text-muted-foreground truncate mt-0.5">
                  <span v-if="c.note">{{ c.note }}</span>
                  <span v-if="c.extraEnvKeys.length" :class="{ 'ml-1.5': c.note }">
                    {{ $t('settings.advancedEnvPrefix') }}{{ c.extraEnvKeys.join('、') }}
                  </span>
                </div>
              </div>
              <button
                class="shrink-0 p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
                :title="$t('common.edit')"
                @click="editing = c"
              >
                <span class="i-carbon-edit w-3.5 h-3.5" />
              </button>
              <button
                class="shrink-0 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                :title="$t('common.delete')"
                @click="onDelete(c)"
              >
                <span class="i-carbon-trash-can w-3.5 h-3.5" />
              </button>
            </div>

            <p v-if="channels.length === 0" class="text-xs text-muted-foreground py-2">
              {{ $t('settings.noChannels') }}
            </p>
          </div>

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
                <select v-model="advisorMain" class="ctrl-select w-full">
                  <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
                  <option value="claude-haiku-4-5">claude-haiku-4-5</option>
                </select>
              </div>
              <div class="setting-cell">
                <div class="setting-label">{{ $t('settings.advisorModel') }}</div>
                <select v-model="advisorModel" class="ctrl-select w-full">
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
              <label class="checkbox-row">
                <input v-model="hideCreditsModels" type="checkbox" />
                <span>{{ $t('settings.hide1mModels') }}</span>
              </label>
              <label class="checkbox-row">
                <input v-model="autoDetectModels" type="checkbox" />
                <span>{{ $t('settings.autoDetect') }}</span>
              </label>
            </div>
          </div>
        </section>

        <!-- ====== Claude Code 配置 ====== -->
        <section v-show="activeTab === 'claude-code'">
          <ClaudeCodeSettings />
        </section>

        <!-- ====== 实验室 ====== -->
        <section v-show="activeTab === 'lab'">
          <h2 class="section-title">{{ $t('settings.lab') }}</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            {{ $t('settings.labDesc') }}
          </p>
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

/* 下拉控件 */
.ctrl-select {
  padding: 5px 8px;
  font-size: 12px;
  font-family: inherit;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  color: var(--foreground);
}
.ctrl-select:focus {
  outline: none;
  border-color: var(--ring);
}
.ctrl-input {
  padding: 5px 8px;
  font-size: 12px;
  font-family: inherit;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  color: var(--foreground);
}
.ctrl-input:focus {
  outline: none;
  border-color: var(--ring);
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

/* 开关 */
.toggle-track {
  position: relative;
  width: 36px;
  height: 20px;
  border-radius: 10px;
  background: var(--muted);
  border: 1px solid var(--border);
  cursor: pointer;
  transition: background 0.2s;
  flex-shrink: 0;
}
.toggle-track.on {
  background: var(--primary);
  border-color: var(--primary);
}
.toggle-knob {
  display: block;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: white;
  position: absolute;
  top: 2px;
  left: 2px;
  transition: transform 0.2s;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
}
.toggle-track.on .toggle-knob {
  transform: translateX(16px);
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

/* 复选框行 */
.checkbox-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
  font-size: 12px;
  cursor: pointer;
}
.checkbox-row input[type="checkbox"] {
  accent-color: var(--primary);
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
