<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { shortModel, relativeTime, formatTokens, type TokenUsage } from '@/types'
import { inferModel, getContextWindow, MODELS } from '@/utils/modelContext'
import { ADVISOR_MAIN_MODEL, type EffortSetting, type EffortLevel } from '@/composables/useSessionSettings'
import { useCliDefaults, refreshCliDefaults } from '@/composables/useCliDefaults'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import { useChannels, OFFICIAL_CHANNEL_ID, channelDisplayName, refreshChannels } from '@/composables/useChannels'
import ModelDropdown from './ModelDropdown.vue'
import ContextProgress from './ContextProgress.vue'
import EffortDropdown from './EffortDropdown.vue'
import ChannelDropdown from './ChannelDropdown.vue'
import type { PermissionMode } from '@/composables/useSessionSettings'

/**
 * 单行极简顶栏:常显区只留高频控件(模型/进度/努力),标题不再显示(列头已有),
 * 元数据(ID/分支/时间)与操作(刷新/终端/VSCode/删除)统一收进 ⋯ 菜单。
 * 窄列折叠的控件也进同一菜单——最窄退化为 [模型][⋯],保持整齐。
 */
const props = defineProps<{
  /** 完整 sessionId(菜单内复制用) */
  sessionId: string
  /** 已计算好的 shortId(菜单内展示) */
  shortIdValue: string
  /** projectId(删除会话用) */
  projectId: string
  /** session.cwd(终端/VSCode 打开用) */
  cwd: string | null
  /** git 分支 */
  gitBranch: string | null
  /** 真实跑过的最近 assistant 消息的 model 字符串(jsonl 里 message.model;含 [1m] 后缀如有) */
  modelString: string | null
  /** 已占用上下文 token 数(最近一次 assistant 响应的 input + cache_read_input_tokens) */
  usedContextTokens: number
  /** API 报告的真实上下文容量（来自 result 事件 modelUsage.contextWindow，null 时回退推断） */
  realContextWindow: number | null
  /** 最后修改时间(秒级时间戳) */
  lastModified: number
  /** 用户已选择的模型 ID(来自 useSessionSettings,可能为 null) */
  selectedModelId: string | null
  /** 用户已选择的努力等级(null = 跟随 CLI,'ultracode' = 超档) */
  selectedEffort: EffortSetting
  /** 渠道选择(null = 跟随应用默认;'official' = 强制官方;其他 = 渠道 id) */
  selectedChannelId: string | null
  /** 解析后的最终注入渠道 id(null = 官方):终端恢复带渠道用 */
  resolvedChannelId: string | null
  /** 顾问模式开关状态 */
  selectedAdvisor: boolean
  /** 权限模式 */
  selectedPermissionMode: PermissionMode
  /** 会话累计 token 用量 */
  totalTokens: TokenUsage
}>()

const emit = defineEmits<{
  (e: 'modelChange', modelId: string): void
  (e: 'effortChange', effort: EffortSetting): void
  (e: 'channelChange', channelId: string | null): void
  (e: 'advisorChange', advisor: boolean): void
  (e: 'permissionModeChange', mode: PermissionMode): void
  (e: 'reload'): void
  (e: 'deleted'): void
}>()

const { t } = useI18n()
const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()

// --- 派生数据 ---

/** 用户在顶栏选的模型(用于下拉显示当前选中);顾问模式锁 Sonnet,未选则用 modelString 真实值 */
const effectiveModelStr = computed(() =>
  props.selectedAdvisor ? ADVISOR_MAIN_MODEL : (props.selectedModelId ?? props.modelString),
)

/** 顾问仅在官方渠道(firstParty)生效:非官方渠道下禁用并提示 */
const advisorDisabled = computed(() => !!props.resolvedChannelId)

/** 顾问开关 title 文案 */
const advisorTitle = computed(() => {
  if (advisorDisabled.value) return t('topbar.advisorDisabled')
  return props.selectedAdvisor
    ? t('topbar.advisorEnabled')
    : t('topbar.advisorEnable')
})

function onAdvisorToggle() {
  if (advisorDisabled.value) return
  emit('advisorChange', !props.selectedAdvisor)
}

/** 解析后的 ModelInfo。API 模型名永远不带 [1m]，无法区分 200K/1M 变体——
 *  有真实容量时按真实值修正；无真实值时默认取 1M 变体（Claude Code CLI 默认 1M） */
const effectiveModel = computed(() => {
  const model = inferModel(effectiveModelStr.value)
  if (!model) return model
  if (model.id.endsWith('[1m]')) return model
  const oneMVariant = MODELS.find(m => m.id === `${model.id}[1m]`)
  if (!oneMVariant) return model
  if (props.realContextWindow) {
    return props.realContextWindow >= oneMVariant.contextWindow ? oneMVariant : model
  }
  return oneMVariant
})

const { cliDefaults } = useCliDefaults()
// 顶栏挂载即拉一次 CLI 默认值(settings.json 活文件,下拉打开时还会各自重读)
onMounted(() => refreshCliDefaults(props.cwd ?? undefined))

/** 上下文容量:API 真值 → effectiveModel 容量 → 模型名推断兜底 */
const capacity = computed(() =>
  props.realContextWindow
    ?? effectiveModel.value?.contextWindow
    ?? getContextWindow(props.selectedModelId ?? cliDefaults.value.model),
)

// --- 窄列折叠 ---
//
// 进度条 flex-1 自适应可压很短,几乎不折叠。
// 折叠顺序:渠道 → 努力等级,模型和进度条永不折叠,顾问始终在菜单。
//   >= 280px : 全部展示
//   >= 200px : 折叠渠道
//   <  200px : 仅模型 + 进度 + 菜单

const containerRef = ref<HTMLElement>()
const containerWidth = ref(Number.POSITIVE_INFINITY)

const showChannel = computed(() => containerWidth.value >= 280)
const showProgress = true
const showEffort = computed(() => containerWidth.value >= 200)

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  const el = containerRef.value
  if (!el) return
  containerWidth.value = el.clientWidth
  resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      containerWidth.value = entry.contentRect.width
    }
  })
  resizeObserver.observe(el)
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
})

// --- ⋯ 统一菜单(折叠控件 + 元数据 + 操作) ---

const menuOpen = ref(false)
const menuRef = ref<HTMLElement>()
const menuPanelRef = ref<HTMLElement>()
const menuAlignLeft = ref(false)

function onDocClick(e: MouseEvent) {
  if (!menuOpen.value) return
  const target = e.target as Node
  if (menuRef.value && !menuRef.value.contains(target)) {
    menuOpen.value = false
    activeSubmenu.value = null
  }
}

function toggleMenu() {
  menuOpen.value = !menuOpen.value
  activeSubmenu.value = null
  if (menuOpen.value) {
    nextTick(() => {
      const panel = menuPanelRef.value
      if (!panel) return
      const rect = panel.getBoundingClientRect()
      menuAlignLeft.value = rect.right > window.innerWidth - 4
    })
  }
}

onMounted(() => document.addEventListener('mousedown', onDocClick))
onUnmounted(() => document.removeEventListener('mousedown', onDocClick))

// --- 二级菜单 ---

const { channels, defaultSessionChannel } = useChannels()

const EFFORT_OPTIONS: { value: NonNullable<EffortSetting>; label: string }[] = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'xhigh', label: 'xHigh' },
  { value: 'max', label: 'Max' },
  { value: 'ultracode', label: 'Ultracode' },
]

const channelOptions = computed(() => {
  const result: { value: string; label: string }[] = [
    { value: OFFICIAL_CHANNEL_ID, label: t('topbar.channelOfficial') },
  ]
  for (const ch of channels.value) {
    if (ch.id !== OFFICIAL_CHANNEL_ID && ch.enabled && ch.scope !== 'agent-only') {
      result.push({ value: ch.id, label: ch.name })
    }
  }
  return result
})

const effortLabel = computed(() => {
  const o = EFFORT_OPTIONS.find(o => o.value === props.selectedEffort)
  return o?.label ?? 'Auto'
})

const channelLabel = computed(() => {
  const id = props.selectedChannelId ?? defaultSessionChannel.value ?? OFFICIAL_CHANNEL_ID
  if (id === OFFICIAL_CHANNEL_ID) return t('topbar.channelOfficial')
  const ch = channels.value.find(c => c.id === id)
  return ch?.name ?? channelDisplayName(id)
})

type SubmenuType = 'effort' | 'channel' | null
const activeSubmenu = ref<SubmenuType>(null)
const submenuAlignLeft = ref(false)

function openSubmenu(type: SubmenuType, event: MouseEvent) {
  activeSubmenu.value = type
  if (type) {
    if (type === 'channel') refreshChannels()
    nextTick(() => {
      const target = (event.currentTarget as HTMLElement)
      const sub = target.querySelector('.submenu-panel') as HTMLElement
      if (!sub) return
      const rect = sub.getBoundingClientRect()
      submenuAlignLeft.value = rect.right > window.innerWidth - 4
    })
  }
}

function selectEffort(value: NonNullable<EffortSetting>) {
  onEffortChange(value)
  menuOpen.value = false
  activeSubmenu.value = null
}

function selectChannel(value: string) {
  onChannelChange(value)
  menuOpen.value = false
  activeSubmenu.value = null
}

// --- 菜单操作 ---

async function copySessionId() {
  try {
    await navigator.clipboard.writeText(props.sessionId)
    notifyTransient(t('topbar.sessionIdCopied'))
  } catch {
    notifyTransient(t('topbar.copyFailed'), t('topbar.checkClipboard'))
  }
  menuOpen.value = false
}

function onReload() {
  menuOpen.value = false
  emit('reload')
}

async function openInFinder() {
  menuOpen.value = false
  if (!props.cwd) return
  try {
    await invoke('open_in_finder', { path: props.cwd })
  } catch (e) {
    notifyTransient(t('topbar.openFailed'), String(e))
  }
}

async function openInTerminal() {
  menuOpen.value = false
  if (!props.cwd) return
  try {
    // 带上会话渠道(--settings <渠道文件>),终端恢复不静默回落官方
    await invoke('resume_in_terminal', {
      cwd: props.cwd,
      sessionId: props.sessionId,
      channel: props.resolvedChannelId,
    })
  } catch (e) {
    notifyTransient(t('topbar.terminalFailed'), String(e))
  }
}

async function openInVscode() {
  menuOpen.value = false
  if (!props.cwd) return
  try {
    await invoke('resume_in_vscode', { cwd: props.cwd })
  } catch (e) {
    notifyTransient(t('topbar.vscodeFailed'), String(e))
  }
}

async function onDelete() {
  menuOpen.value = false
  const ok = await confirm(t('archive.deleteSessionConfirm'), t('common.delete'))
  if (!ok) return
  try {
    await invoke('delete_session', { projectId: props.projectId, sessionId: props.sessionId })
    emit('deleted')
  } catch (e) {
    notifyTransient(t('topbar.deleteFailed'), String(e))
  }
}

// --- 事件转发 ---

function onModelChange(id: string) {
  emit('modelChange', id)
}
function onEffortChange(level: EffortSetting) {
  emit('effortChange', level)
}
function onChannelChange(channelId: string | null) {
  emit('channelChange', channelId)
}
function onPermissionModeChange(mode: PermissionMode) {
  emit('permissionModeChange', mode)
}
</script>

<template>
  <div
    ref="containerRef"
    class="px-3 py-1 border-b border-border shrink-0 flex items-center gap-1.5"
  >
    <!-- 模型切换(永不折叠);渠道决定候选来源(官方=角色主区;第三方=映射角色) -->
    <ModelDropdown
      :current="effectiveModel?.id ?? effectiveModelStr"
      :channel="resolvedChannelId"
      :disabled="selectedAdvisor"
      @select="onModelChange"
    />

    <!-- 努力等级 -->
    <EffortDropdown
      v-if="showEffort"
      :current="selectedEffort"
      @select="onEffortChange"
    />

    <!-- 渠道 -->
    <ChannelDropdown
      v-if="showChannel"
      :current="selectedChannelId"
      @select="onChannelChange"
    />

    <!-- token 进度(紧凑形态:条 + 百分比,永不折叠) -->
    <ContextProgress
      :used="usedContextTokens"
      :capacity="capacity"
      compact
    />

    <!-- ⋯ 统一菜单 -->
    <div ref="menuRef" class="relative inline-flex shrink-0">
      <button
        type="button"
        class="p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        :title="$t('topbar.sessionMenu')"
        :aria-expanded="menuOpen"
        @click="toggleMenu"
      >
        <span class="i-carbon-overflow-menu-horizontal w-3.5 h-3.5" />
      </button>

      <div
        v-if="menuOpen"
        ref="menuPanelRef"
        class="absolute top-full mt-1 z-50 py-1 rounded-md border border-border
               shadow-paper-lifted bg-popover w-52"
        :class="menuAlignLeft ? 'left-0' : 'right-0'"
      >
        <!-- 折叠的下拉控件(二级菜单) + 进度 + 顾问 -->
        <div class="py-0.5 border-b border-border">
          <!-- 努力等级(二级菜单) -->
          <div
            v-if="!showEffort"
            class="submenu-trigger"
            @mouseenter="openSubmenu('effort', $event)"
            @mouseleave="activeSubmenu === 'effort' && (activeSubmenu = null)"
          >
            <span class="menu-item">
              <span class="i-carbon-meter w-3.5 h-3.5" />
              <span class="flex-1">{{ $t('topbar.effortLabel') }}</span>
              <span class="text-muted-foreground">{{ effortLabel }}</span>
              <span class="i-carbon-chevron-right w-3 h-3 text-muted-foreground" />
            </span>
            <div
              v-if="activeSubmenu === 'effort'"
              class="submenu-panel"
              :class="submenuAlignLeft ? 'right-full mr-1' : 'left-full ml-1'"
            >
              <button
                v-for="o in EFFORT_OPTIONS"
                :key="o.value"
                class="menu-item"
                :class="{ 'text-primary!': o.value === selectedEffort }"
                @click="selectEffort(o.value)"
              >
                <span
                  class="w-3 h-3 shrink-0"
                  :class="o.value === selectedEffort ? 'i-carbon-checkmark text-primary' : ''"
                />
                {{ o.label }}
              </button>
            </div>
          </div>

          <!-- 渠道(二级菜单) -->
          <div
            v-if="!showChannel"
            class="submenu-trigger"
            @mouseenter="openSubmenu('channel', $event)"
            @mouseleave="activeSubmenu === 'channel' && (activeSubmenu = null)"
          >
            <span class="menu-item">
              <span class="i-carbon-cloud w-3.5 h-3.5" />
              <span class="flex-1">{{ $t('topbar.channelLabel') }}</span>
              <span class="text-muted-foreground">{{ channelLabel }}</span>
              <span class="i-carbon-chevron-right w-3 h-3 text-muted-foreground" />
            </span>
            <div
              v-if="activeSubmenu === 'channel'"
              class="submenu-panel"
              :class="submenuAlignLeft ? 'right-full mr-1' : 'left-full ml-1'"
            >
              <button
                v-for="o in channelOptions"
                :key="o.value"
                class="menu-item"
                :class="{ 'text-primary!': o.value === (selectedChannelId ?? defaultSessionChannel ?? OFFICIAL_CHANNEL_ID) }"
                @click="selectChannel(o.value)"
              >
                <span
                  class="w-3 h-3 shrink-0"
                  :class="o.value === (selectedChannelId ?? defaultSessionChannel ?? OFFICIAL_CHANNEL_ID) ? 'i-carbon-checkmark text-primary' : ''"
                />
                {{ o.label }}
              </button>
            </div>
          </div>

          <!-- 顾问(始终在菜单) -->
          <button
            class="menu-item"
            :disabled="advisorDisabled"
            :class="advisorDisabled ? 'opacity-40 cursor-not-allowed' : ''"
            :title="advisorTitle"
            @click="onAdvisorToggle"
          >
            <span class="i-carbon-idea w-3.5 h-3.5" />
            <span class="flex-1">{{ $t('topbar.advisorMode') }}</span>
            <span
              v-if="selectedAdvisor && !advisorDisabled"
              class="i-carbon-checkmark w-3 h-3 text-primary"
            />
          </button>
        </div>

        <!-- 元数据 -->
        <div class="px-3 py-1.5 text-xs text-muted-foreground flex flex-col gap-1 border-b border-border">
          <button
            class="flex items-center gap-1.5 hover:text-foreground transition-colors text-left"
            :title="$t('topbar.copySessionId')"
            @click="copySessionId"
          >
            <span class="font-mono">{{ shortIdValue }}</span>
            <span class="i-carbon-copy w-3 h-3 shrink-0" />
          </button>
          <span v-if="gitBranch" class="flex items-center gap-1.5">
            <span class="i-carbon-branch w-3 h-3 shrink-0" />{{ gitBranch }}
          </span>
          <span v-if="modelString && !effectiveModel" class="truncate">{{ shortModel(modelString) }}</span>
          <span>{{ relativeTime(lastModified) }}</span>
          <!-- 会话 token 统计 -->
          <div class="flex flex-col gap-0.5 pt-1 border-t border-border/50 tabular-nums">
            <!-- 原始四项 -->
            <div class="flex items-center justify-between">
              <span>input_tokens</span>
              <span>{{ formatTokens(totalTokens.input_tokens) }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span>output_tokens</span>
              <span>{{ formatTokens(totalTokens.output_tokens) }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span>cache_creation</span>
              <span>{{ formatTokens(totalTokens.cache_creation_input_tokens) }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span>cache_read</span>
              <span>{{ formatTokens(totalTokens.cache_read_input_tokens) }}</span>
            </div>
            <!-- 派生指标 -->
            <div class="flex items-center justify-between pt-1 border-t border-border/50">
              <span>{{ $t('topbar.tokenTotalInput') }}</span>
              <span>{{ formatTokens(totalTokens.input_tokens + totalTokens.cache_creation_input_tokens + totalTokens.cache_read_input_tokens) }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span>{{ $t('topbar.tokenTotalOutput') }}</span>
              <span>{{ formatTokens(totalTokens.output_tokens) }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span>{{ $t('topbar.tokenCacheHitRate') }}</span>
              <span>{{ (totalTokens.input_tokens + totalTokens.cache_read_input_tokens + totalTokens.cache_creation_input_tokens) > 0 ? Math.round(totalTokens.cache_read_input_tokens / (totalTokens.input_tokens + totalTokens.cache_read_input_tokens + totalTokens.cache_creation_input_tokens) * 100) + '%' : '—' }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span>{{ $t('topbar.tokenCacheRatio') }}</span>
              <span>{{ (totalTokens.input_tokens + totalTokens.output_tokens + totalTokens.cache_read_input_tokens + totalTokens.cache_creation_input_tokens) > 0 ? Math.round(totalTokens.cache_read_input_tokens / (totalTokens.input_tokens + totalTokens.output_tokens + totalTokens.cache_read_input_tokens + totalTokens.cache_creation_input_tokens) * 100) + '%' : '—' }}</span>
            </div>
            <div class="flex items-center justify-between font-medium text-foreground">
              <span>{{ $t('topbar.tokenTotal') }}</span>
              <span>{{ formatTokens(totalTokens.input_tokens + totalTokens.output_tokens + totalTokens.cache_read_input_tokens + totalTokens.cache_creation_input_tokens) }}</span>
            </div>
          </div>
        </div>

        <!-- 操作 -->
        <div class="py-1 flex flex-col">
          <button class="menu-item" @click="onReload">
            <span class="i-carbon-renew w-3.5 h-3.5" />{{ $t('topbar.refreshSession') }}
          </button>
          <button v-if="cwd" class="menu-item" @click="openInFinder">
            <span class="i-carbon-folder w-3.5 h-3.5" />{{ $t('topbar.openInFinder') }}
          </button>
          <button v-if="cwd" class="menu-item" @click="openInTerminal">
            <span class="i-carbon-terminal w-3.5 h-3.5" />{{ $t('topbar.openInTerminal') }}
          </button>
          <button v-if="cwd" class="menu-item" @click="openInVscode">
            <span class="i-carbon-code w-3.5 h-3.5" />{{ $t('topbar.openInVscode') }}
          </button>
          <button class="menu-item text-destructive! hover:bg-destructive/10" @click="onDelete">
            <span class="i-carbon-trash-can w-3.5 h-3.5" />{{ $t('topbar.deleteSession') }}
          </button>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
.menu-item {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 5px 12px;
  font-size: 12px;
  text-align: left;
  color: var(--foreground);
  transition: background-color 0.15s;
}
.menu-item:hover {
  background: var(--muted);
}
.submenu-trigger {
  position: relative;
}
.submenu-panel {
  position: absolute;
  top: 0;
  z-index: 51;
  min-width: 120px;
  padding: 4px 0;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--popover);
  box-shadow: var(--shadow-paper-lifted);
}
</style>
