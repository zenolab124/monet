<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import {
  useChannels,
  refreshChannels,
  OFFICIAL_CHANNEL_ID,
  type ChannelInfo,
} from '@/composables/useChannels'
import { useUiState } from '@/composables/useUiState'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import { useHomeStats } from '@/composables/useHomeStats'
import ChannelForm from '@/components/settings/ChannelForm.vue'
import DiagnosisCard from '@/components/home/DiagnosisCard.vue'
import AgentIframeDemo from '@/components/settings/AgentIframeDemo.vue'
import ClaudeCodeSettings from '@/components/settings/ClaudeCodeSettings.vue'

const { channels, defaultChannelId, deleteChannel, setDefaultChannel, revealChannelsDir } =
  useChannels()
const { activeSection } = useUiState()
const { diag, diagLoading, diagError, diagAt, retryDiag, ensureLoaded } = useHomeStats()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()

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
    `删除渠道「${ch.name}」?将删除 channels/${ch.id}.json 文件，不可恢复。`,
    '删除',
  )
  if (!ok) return
  try {
    await deleteChannel(ch.id)
    if (editing.value !== 'new' && editing.value?.id === ch.id) editing.value = null
    notifyTransient('渠道已删除')
  } catch (e) {
    notifyTransient('删除失败', String(e))
  }
}

async function onDefaultChange(e: Event) {
  const value = (e.target as HTMLSelectElement).value
  try {
    await setDefaultChannel(value === OFFICIAL_CHANNEL_ID ? null : value)
  } catch (err) {
    notifyTransient('设置默认渠道失败', String(err))
    await refreshChannels()
  }
}

async function onReveal() {
  try {
    await revealChannelsDir()
  } catch (e) {
    notifyTransient('打开目录失败', String(e))
  }
}

function onSaved() {
  editing.value = null
  notifyTransient('渠道已保存')
}
</script>

<template>
  <div class="h-full p-2.5">
    <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">
    <!-- 侧栏导航 -->
    <nav class="side-nav">
      <h1 class="side-title">
        <span class="i-carbon-settings w-4 h-4 opacity-70" />设置
      </h1>
      <button :class="['side-item', { active: activeTab === 'general' }]" @click="activeTab = 'general'">
        <span class="i-carbon-settings-adjust w-3.5 h-3.5" />常规
      </button>
      <button :class="['side-item', { active: activeTab === 'channels' }]" @click="activeTab = 'channels'">
        <span class="i-carbon-connect w-3.5 h-3.5" />渠道
      </button>
      <button :class="['side-item', { active: activeTab === 'models' }]" @click="activeTab = 'models'">
        <span class="i-carbon-bot w-3.5 h-3.5" />模型
      </button>
      <button :class="['side-item', { active: activeTab === 'claude-code' }]" @click="activeTab = 'claude-code'">
        <span class="i-carbon-json w-3.5 h-3.5" />Claude Code
      </button>
      <button :class="['side-item', { active: activeTab === 'lab' }]" @click="activeTab = 'lab'">
        <span class="i-carbon-chemistry w-3.5 h-3.5" />实验室
      </button>
      <button :class="['side-item', { active: activeTab === 'diag' }]" @click="activeTab = 'diag'">
        <span class="i-carbon-debug w-3.5 h-3.5" />诊断
      </button>
    </nav>

    <!-- 内容区 -->
    <div class="flex-1 min-w-0 overflow-y-auto">
      <div class="px-6 py-5 max-w-2xl">

        <!-- ====== 常规 ====== -->
        <section v-show="activeTab === 'general'">
          <h2 class="section-title">常规</h2>

          <div class="setting-row">
            <div class="setting-label">
              默认努力等级
              <div class="setting-hint">无 per-session 记录的会话发送档位</div>
            </div>
            <select v-model="defaultEffort" class="ctrl-select">
              <option value="cli">跟随 CLI</option>
              <option value="low">Low</option>
              <option value="medium">Medium</option>
              <option value="high">High</option>
              <option value="xhigh">xHigh</option>
              <option value="max">Max</option>
              <option value="ultracode">Ultracode</option>
            </select>
          </div>

          <div class="setting-row">
            <div class="setting-label">
              Remote Control
              <div class="setting-hint">新会话自动启用远程操控</div>
            </div>
            <div class="flex items-center gap-2.5">
              <button
                :class="['toggle-track', { on: rcEnabled }]"
                @click="rcEnabled = !rcEnabled"
              >
                <span class="toggle-knob" />
              </button>
              <span class="text-[11px] text-muted-foreground">登录同账号设备可见</span>
            </div>
          </div>
        </section>

        <!-- ====== 渠道 ====== -->
        <section v-show="activeTab === 'channels'">
          <h2 class="section-title">渠道</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            每个渠道是一份标准 Claude Code settings 格式的 JSON 文件，发送消息时经
            <span class="font-mono">--settings</span> 注入。
            高级 env 可直接手编文件，本页改动不会抹掉手编字段。
          </p>

          <div class="setting-row" style="border-top:none; padding-top:0;">
            <div class="setting-label">默认渠道</div>
            <select
              :value="defaultChannelId ?? OFFICIAL_CHANNEL_ID"
              class="ctrl-select"
              @change="onDefaultChange"
            >
              <option :value="OFFICIAL_CHANNEL_ID">官方 (不注入，沿用登录态)</option>
              <option v-for="c in channels" :key="c.id" :value="c.id">{{ c.name }}</option>
            </select>
          </div>

          <!-- 渠道列表 -->
          <div class="flex flex-col gap-2 mb-3">
            <div
              v-for="c in channels"
              :key="c.id"
              class="rounded-md border border-border bg-card px-3 py-2 flex items-center gap-3"
            >
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5 text-xs">
                  <span class="font-medium truncate">{{ c.name }}</span>
                  <span class="text-muted-foreground font-mono">{{ c.id }}</span>
                  <span v-if="c.isDefault" class="channel-chip">默认</span>
                  <span v-if="!c.valid" class="channel-chip text-destructive border-destructive">JSON 解析失败</span>
                </div>
                <div class="text-[11px] text-muted-foreground truncate mt-0.5 font-mono">
                  {{ c.baseUrl ?? '(未配置 ANTHROPIC_BASE_URL)' }}
                  <span v-if="c.authTokenMasked" class="ml-1.5">token {{ c.authTokenMasked }}</span>
                </div>
                <div v-if="c.extraEnvKeys.length || c.note" class="text-[11px] text-muted-foreground truncate mt-0.5">
                  <span v-if="c.note">{{ c.note }}</span>
                  <span v-if="c.extraEnvKeys.length" :class="{ 'ml-1.5': c.note }">
                    高级 env: {{ c.extraEnvKeys.join('、') }}
                  </span>
                </div>
              </div>
              <button
                class="shrink-0 p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
                title="编辑"
                @click="editing = c"
              >
                <span class="i-carbon-edit w-3.5 h-3.5" />
              </button>
              <button
                class="shrink-0 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                title="删除"
                @click="onDelete(c)"
              >
                <span class="i-carbon-trash-can w-3.5 h-3.5" />
              </button>
            </div>

            <p v-if="channels.length === 0" class="text-xs text-muted-foreground py-2">
              尚无渠道。新增后即可在会话顶栏按会话切换。
            </p>
          </div>

          <ChannelForm
            v-if="editing"
            :key="editing === 'new' ? '__new__' : editing.id"
            :channel="editing === 'new' ? null : editing"
            class="mb-3"
            @saved="onSaved"
            @cancel="editing = null"
          />

          <div class="flex items-center gap-2">
            <button
              v-if="!editing"
              class="px-2.5 py-1 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow"
              @click="editing = 'new'"
            >
              + 新增渠道
            </button>
            <button
              class="px-2.5 py-1 text-xs rounded-md text-muted-foreground border border-border hover:text-foreground hover:bg-muted transition-colors"
              @click="onReveal"
            >
              打开配置目录
            </button>
          </div>
        </section>

        <!-- ====== 模型 ====== -->
        <section v-show="activeTab === 'models'">
          <h2 class="section-title">模型</h2>

          <!-- 顾问模式 -->
          <div class="sub-card">
            <h3 class="sub-card-title">顾问模式</h3>
            <div class="setting-row" style="border-top:none; padding-top:0;">
              <div class="setting-label">主模型</div>
              <select v-model="advisorMain" class="ctrl-select">
                <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
                <option value="claude-haiku-4-5">claude-haiku-4-5</option>
              </select>
            </div>
            <div class="setting-row">
              <div class="setting-label">顾问模型</div>
              <select v-model="advisorModel" class="ctrl-select">
                <option value="claude-fable-5">claude-fable-5</option>
                <option value="claude-opus-4-8">claude-opus-4-8</option>
                <option value="claude-opus-4-6">claude-opus-4-6</option>
              </select>
            </div>
            <p class="text-[11px] text-accent mt-1">⚠ 顾问等级须 ≥ 主模型 · 仅官方渠道生效</p>
          </div>

          <!-- 模型可见性 -->
          <div class="sub-card">
            <h3 class="sub-card-title">模型可见性</h3>
            <label class="checkbox-row">
              <input v-model="hideCreditsModels" type="checkbox" />
              <span>隐藏需 usage credits 的 1M 档位</span>
            </label>
            <label class="checkbox-row">
              <input v-model="autoDetectModels" type="checkbox" />
              <span>自动探测可用性（选到不可用档位后置灰）</span>
            </label>
          </div>
        </section>

        <!-- ====== Claude Code 配置 ====== -->
        <section v-show="activeTab === 'claude-code'">
          <ClaudeCodeSettings />
        </section>

        <!-- ====== 实验室 ====== -->
        <section v-show="activeTab === 'lab'">
          <h2 class="section-title">实验室</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            验证 Tauri webview 中 iframe + postMessage 双向通信链路。
          </p>
          <div class="iframe-zone">
            <span class="iframe-badge">IFRAME</span>
            <AgentIframeDemo />
          </div>
        </section>

        <!-- ====== 诊断 ====== -->
        <section v-show="activeTab === 'diag'">
          <h2 class="section-title">诊断</h2>
          <p class="text-xs text-muted-foreground mb-3 leading-relaxed">
            扫描所有会话文件，检查记录类型和工具的解析覆盖率。
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

/* 分区标题 */
.section-title {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 14px;
}

/* 设置行 */
.setting-row {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  padding: 10px 0;
}
.setting-row + .setting-row {
  border-top: 1px solid var(--border);
}
.setting-label {
  flex: 0 0 130px;
  font-size: 12px;
  font-weight: 500;
}
.setting-hint {
  font-size: 11px;
  color: var(--muted-foreground);
  font-weight: 400;
  margin-top: 2px;
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
  max-width: 240px;
}
.ctrl-select:focus {
  outline: none;
  border-color: var(--ring);
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
