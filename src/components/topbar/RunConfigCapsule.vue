<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SessionSettings, EffortSetting, EffortLevel } from '@/composables/useSessionSettings'
import type { ResolvedRunConfig } from '@/composables/useRunConfig'
import {
  useChannels,
  refreshChannels,
  OFFICIAL_CHANNEL_ID,
} from '@/composables/useChannels'
import { useModelOptions } from '@/composables/useModelOptions'
import { useCliDefaults, refreshCliDefaults } from '@/composables/useCliDefaults'
import { useUiState } from '@/composables/useUiState'
import { inferModel, effortCapabilities } from '@/utils/modelContext'

/**
 * 运行配置胶囊(二期,原型冻结基准 docs/prototypes/run-config-capsule.html):
 * 渠道/模型/强度三段一枚胶囊,常显解析值(与发送同源);点哪段从哪层开渐进面板——
 * 强度一列 / 模型两列 / 渠道三列全景(窄列渠道段收起,点任意段直接开全景)。
 * 面板列表只放纯候选项:选中 = 解析值所在项;默认值右侧小字「默认」;
 * 不支持档右侧小字「不支持」(软提示不拦截,能力名单可能随 CLI 更新过时);
 * 覆盖态列头出「重置」清覆盖回跟随。
 */
const props = defineProps<{
  /** 会话覆盖原值(重置钮显隐/顾问开关状态判定) */
  settings: Pick<SessionSettings, 'modelId' | 'effort' | 'channelId' | 'advisor'>
  runConfig: ResolvedRunConfig
  /** 窄列:胶囊收起渠道段,点任意段开全景 */
  narrow?: boolean
}>()

const emit = defineEmits<{
  (e: 'modelChange', modelId: string | null): void
  (e: 'effortChange', effort: EffortSetting): void
  (e: 'channelChange', channelId: string | null): void
  (e: 'advisorChange', advisor: boolean): void
}>()

const { t } = useI18n()
const { channels, defaultSessionChannel } = useChannels()
const { cliDefaults } = useCliDefaults()

// ---- 面板开合(渐进层级) ----

type Layer = 'effort' | 'model' | 'channel'
const openLayer = ref<Layer | null>(null)
const containerRef = ref<HTMLElement>()

/** 面板显示哪些列(自左向右) */
const visibleCols = computed<Layer[]>(() => {
  switch (openLayer.value) {
    case 'channel': return ['channel', 'model', 'effort']
    case 'model': return ['model', 'effort']
    case 'effort': return ['effort']
    default: return []
  }
})

function openFrom(layer: Layer) {
  // 窄列渠道段收起:点任意段直接开全景,规则简化
  const target = props.narrow ? 'channel' : layer
  openLayer.value = openLayer.value === target ? null : target
  if (openLayer.value) {
    // 渠道文件/settings.json 都是活文件:开面板即重读,不显示过期值
    refreshChannels()
    refreshCliDefaults()
  }
}

function onDocumentClick(e: MouseEvent) {
  if (!openLayer.value) return
  const target = e.target as Node
  if (containerRef.value && !containerRef.value.contains(target)) {
    openLayer.value = null
  }
}
onMounted(() => document.addEventListener('mousedown', onDocumentClick))
onUnmounted(() => document.removeEventListener('mousedown', onDocumentClick))

// ---- 渠道列 ----

const channelOptions = computed(() => {
  const result: { id: string; name: string }[] = [
    { id: OFFICIAL_CHANNEL_ID, name: t('topbar.channelOfficial') },
  ]
  for (const ch of channels.value) {
    if (ch.id !== OFFICIAL_CHANNEL_ID && ch.enabled && ch.scope !== 'agent-only') {
      result.push({ id: ch.id, name: ch.name })
    }
  }
  return result
})

/** 解析后的当前渠道(选中判定;null=官方) */
const resolvedChannelKey = computed(() => props.runConfig.channelId ?? OFFICIAL_CHANNEL_ID)
/** 应用默认渠道(「默认」hint 所在项) */
const defaultChannelKey = computed(() => {
  const id = defaultSessionChannel.value
  if (!id) return OFFICIAL_CHANNEL_ID
  const ch = channels.value.find(c => c.id === id)
  return ch && ch.enabled ? id : OFFICIAL_CHANNEL_ID
})
const channelOverridden = computed(() => props.settings.channelId !== null)

function pickChannel(id: string) {
  // 与 ChannelDropdown 语义一致:点选即会话指定(含 official 强制态)
  emit('channelChange', id)
}

// ---- 模型列 ----

const channelRef = computed<string | null>(() => props.runConfig.channelId)
const { items: modelItems } = useModelOptions(channelRef)

/** 当前渠道的 modelEnv(能力声明判定用) */
const activeModelEnv = computed<Record<string, string> | undefined>(() => {
  const id = props.runConfig.channelId
  if (!id) return undefined
  return channels.value.find(c => c.id === id)?.modelEnv
})

/** 把模型字符串归位到候选项 id(选中/默认 hint 判定) */
function modelKeyOf(modelStr: string | null | undefined): string | null {
  if (!modelStr) return null
  const lower = modelStr.toLowerCase()
  if (modelItems.value.some(m => m.id === lower)) return lower
  const inferred = inferModel(lower)
  if (inferred && modelItems.value.some(m => m.id === inferred.id)) return inferred.id
  return lower
}

const selectedModelKey = computed(() => modelKeyOf(props.runConfig.model ?? cliDefaults.value.model))
const defaultModelKey = computed(() =>
  modelKeyOf(props.runConfig.channelDefaultModel ?? cliDefaults.value.model),
)
const modelOverridden = computed(() => props.runConfig.modelSource === 'session')
const advisorLocked = computed(() => props.runConfig.modelSource === 'advisor')

/** 会话在用清单外模型时附加为候选(原名展示,与旧 ModelDropdown 行为一致) */
const modelListItems = computed(() => {
  const base = modelItems.value
  const sel = selectedModelKey.value
  if (sel && !base.some(m => m.id === sel)) {
    return [...base, { id: sel, label: props.runConfig.model ?? sel, contextWindow: 0 }]
  }
  return base
})

function pickModel(id: string) {
  if (advisorLocked.value) return
  emit('modelChange', id)
}

// ---- 强度列 ----

const EFFORT_LABELS: Record<EffortLevel, string> = {
  low: 'Low', medium: 'Medium', high: 'High', xhigh: 'xHigh', max: 'Max',
}
const EFFORT_OPTIONS: { value: NonNullable<EffortSetting>; label: string }[] = [
  ...(Object.entries(EFFORT_LABELS) as [EffortLevel, string][]).map(([value, label]) => ({ value, label })),
  { value: 'ultracode' as const, label: 'Ultracode' },
]

/** CLI 层读数(ultracode 独立开关生效时覆盖 effortLevel) */
const cliEffortValue = computed<NonNullable<EffortSetting> | null>(() => {
  if (cliDefaults.value.ultracode) return 'ultracode'
  const lv = cliDefaults.value.effort_level
  return lv && lv in EFFORT_LABELS ? (lv as EffortLevel) : null
})

const selectedEffort = computed<NonNullable<EffortSetting> | null>(
  () => props.runConfig.effort ?? cliEffortValue.value,
)
const defaultEffort = computed<NonNullable<EffortSetting> | null>(
  () => props.runConfig.channelDefaultEffort ?? cliEffortValue.value,
)
const effortOverridden = computed(() => props.runConfig.effortSource === 'session')

/** 强度能力标注:基于当前解析模型 + 渠道声明(软提示,不拦截) */
const effortCaps = computed(() =>
  effortCapabilities(props.runConfig.model ?? cliDefaults.value.model, activeModelEnv.value),
)
function effortUnsupported(value: NonNullable<EffortSetting>): boolean {
  if (value === 'xhigh') return effortCaps.value.xhigh === false
  if (value === 'max') return effortCaps.value.max === false
  if (value === 'ultracode') return effortCaps.value.ultracode === false
  return false
}

function pickEffort(value: NonNullable<EffortSetting>) {
  emit('effortChange', value)
}

// ---- 胶囊段显示 ----

const channelSegLabel = computed(() => {
  const id = props.runConfig.channelId
  if (!id) return t('topbar.channelOfficial')
  return channels.value.find(c => c.id === id)?.name ?? id
})

const modelSegLabel = computed(() => {
  const resolved = props.runConfig.model
  if (resolved) {
    const key = modelKeyOf(resolved)
    const hit = key ? modelListItems.value.find(m => m.id === key) : null
    return hit?.label ?? resolved
  }
  const m = cliDefaults.value.model
  if (m) return inferModel(m)?.label ?? m
  return t('topbar.modelDefault')
})

const effortSegLabel = computed(() => {
  const v = selectedEffort.value
  if (!v) return 'High'
  return EFFORT_OPTIONS.find(o => o.value === v)?.label ?? v
})

/** 来源列头文案 */
function srcLabel(source: string): string {
  switch (source) {
    case 'session': return t('topbar.srcSession')
    case 'channel': return t('topbar.srcChannel')
    case 'advisor': return t('topbar.srcAdvisor')
    default: return t('topbar.srcCli')
  }
}
const channelSrcLabel = computed(() =>
  channelOverridden.value ? t('topbar.srcSession') : t('topbar.srcApp'),
)

// ---- 顾问 ----
const advisorDisabled = computed(() => !!props.runConfig.channelId)
function onAdvisorToggle() {
  if (advisorDisabled.value) return
  emit('advisorChange', !props.settings.advisor)
}

// ---- 管理渠道入口(原 ChannelDropdown 功能保留) ----
const { switchSection } = useUiState()
function openSettings() {
  openLayer.value = null
  switchSection('settings')
}
</script>

<template>
  <div ref="containerRef" class="relative inline-flex min-w-0">
    <!-- 胶囊:三段一枚,hover 段高亮,点哪段开哪层 -->
    <div
      class="inline-flex items-center h-[22px] border border-border rounded-[5px] text-xs
             text-muted-foreground cursor-pointer select-none whitespace-nowrap overflow-hidden min-w-0"
    >
      <button
        v-if="!narrow"
        type="button"
        class="capsule-seg"
        :class="settings.channelId !== null ? 'seg-overridden' : 'seg-inherited'"
        :title="$t('topbar.channelTitle', { name: channelSegLabel })"
        @click="openFrom('channel')"
      >{{ channelSegLabel }}</button>
      <button
        type="button"
        class="capsule-seg seg-sep"
        :class="[
          runConfig.modelSource === 'session' || runConfig.modelSource === 'advisor' ? 'seg-overridden' : 'seg-inherited',
          narrow ? 'seg-first' : '',
        ]"
        :title="$t('topbar.modelTitle', { name: modelSegLabel })"
        @click="openFrom('model')"
      >{{ modelSegLabel }}</button>
      <button
        type="button"
        class="capsule-seg seg-sep"
        :class="effortOverridden ? 'seg-overridden' : 'seg-inherited'"
        :title="$t('topbar.effortTitle', { name: effortSegLabel })"
        @click="openFrom('effort')"
      >{{ effortSegLabel }}</button>
    </div>

    <!-- 渐进面板:列式扩展 -->
    <div
      v-if="openLayer"
      class="absolute top-full left-0 mt-1 z-50 inline-flex rounded-md border border-border
             shadow-paper-lifted bg-popover"
    >
      <!-- 渠道列 -->
      <div v-if="visibleCols.includes('channel')" class="rc-col">
        <div class="rc-head">
          <span class="rc-label">{{ $t('topbar.channelLabel') }}</span>
          <span class="rc-src">{{ channelSrcLabel }}</span>
          <button v-if="channelOverridden" class="rc-reset" @click="emit('channelChange', null)">{{ $t('topbar.resetInherit') }}</button>
        </div>
        <div class="rc-list">
          <button
            v-for="ch in channelOptions"
            :key="ch.id"
            class="rc-opt"
            :class="{ sel: ch.id === resolvedChannelKey }"
            @click="pickChannel(ch.id)"
          >
            <span class="truncate">{{ ch.name }}</span>
            <span v-if="ch.id === defaultChannelKey" class="rc-hint">{{ $t('topbar.hintDefault') }}</span>
          </button>
        </div>
        <button class="rc-opt rc-manage" @click="openSettings">
          <span class="i-carbon-settings w-3 h-3 shrink-0" />
          <span>{{ $t('topbar.manageChannels') }}</span>
        </button>
        <div class="rc-advisor" :class="{ 'opacity-45': advisorDisabled }">
          <button
            type="button"
            :class="['form-toggle-sm', { on: settings.advisor && !advisorDisabled }]"
            :disabled="advisorDisabled"
            :title="advisorDisabled ? $t('topbar.advisorDisabled') : ''"
            @click="onAdvisorToggle"
          ><span class="form-toggle-knob" /></button>
          <span>{{ $t('topbar.advisorMode') }}</span>
        </div>
        <div class="rc-foot">{{ $t('topbar.advisorFoot') }}</div>
      </div>

      <!-- 模型列 -->
      <div v-if="visibleCols.includes('model')" class="rc-col">
        <div class="rc-head">
          <span class="rc-label">{{ $t('topbar.modelLabel') }}</span>
          <span class="rc-src">{{ advisorLocked ? $t('topbar.srcAdvisor') : srcLabel(runConfig.modelSource) }}</span>
          <button v-if="modelOverridden" class="rc-reset" @click="emit('modelChange', null)">{{ $t('topbar.resetInherit') }}</button>
        </div>
        <div class="rc-list" :class="{ 'opacity-45 pointer-events-none': advisorLocked }" :title="advisorLocked ? $t('topbar.modelAdvisorLocked') : ''">
          <template v-for="(m, i) in modelListItems" :key="m.id">
            <div v-if="i > 0 && !!m.legacy !== !!modelListItems[i - 1].legacy" class="rc-divider" />
            <button
              class="rc-opt"
              :class="{ sel: m.id === selectedModelKey, 'opacity-55': m.legacy }"
              @click="pickModel(m.id)"
            >
              <span class="truncate">{{ m.label }}</span>
              <span v-if="m.id === defaultModelKey" class="rc-hint">{{ $t('topbar.hintDefault') }}</span>
            </button>
          </template>
        </div>
      </div>

      <!-- 强度列 -->
      <div v-if="visibleCols.includes('effort')" class="rc-col">
        <div class="rc-head">
          <span class="rc-label">{{ $t('topbar.effortLabel') }}</span>
          <span class="rc-src">{{ srcLabel(runConfig.effortSource) }}</span>
          <button v-if="effortOverridden" class="rc-reset" @click="emit('effortChange', null)">{{ $t('topbar.resetInherit') }}</button>
        </div>
        <div class="rc-list">
          <button
            v-for="o in EFFORT_OPTIONS"
            :key="o.value"
            class="rc-opt"
            :title="effortUnsupported(o.value) ? $t('topbar.effortUnsupportedTip') : ''"
            :class="{ sel: o.value === selectedEffort }"
            @click="pickEffort(o.value)"
          >
            <span>{{ o.label }}</span>
            <span v-if="o.value === defaultEffort" class="rc-hint">{{ $t('topbar.hintDefault') }}</span>
            <span v-else-if="effortUnsupported(o.value)" class="rc-hint rc-warn">{{ $t('topbar.hintUnsupported') }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.capsule-seg {
  padding: 0 7px;
  height: 100%;
  display: inline-flex;
  align-items: center;
  position: relative;
  transition: background .12s, color .12s;
  max-width: 9rem;
  overflow: hidden;
  text-overflow: ellipsis;
}
.capsule-seg:hover { background: var(--muted); color: var(--foreground); }
.seg-sep::before {
  content: '·';
  position: absolute;
  left: -2.5px;
  opacity: .4;
  pointer-events: none;
}
.seg-first::before { content: none; }
.seg-inherited { opacity: .68; }
.seg-overridden { color: var(--foreground); font-weight: 500; }

.rc-col { width: 152px; padding: 8px 8px 6px; display: flex; flex-direction: column; }
.rc-col + .rc-col { border-left: 1px solid var(--border); }
.rc-head { display: flex; align-items: baseline; gap: 6px; margin-bottom: 5px; padding: 0 2px; }
.rc-label { font-size: 11px; color: var(--muted-foreground); font-weight: 500; }
.rc-src { font-size: 9px; color: var(--muted-foreground); opacity: .7; margin-left: auto; }
.rc-reset { font-size: 9px; color: var(--primary); cursor: pointer; }
.rc-list { display: flex; flex-direction: column; }
.rc-opt {
  font-size: 12px; padding: 3px 8px; border-radius: 4px; color: var(--muted-foreground);
  cursor: pointer; display: flex; align-items: center; gap: 5px; margin-bottom: 1px;
  text-align: left; width: 100%;
}
.rc-opt:hover { background: var(--muted); color: var(--foreground); }
.rc-opt.sel { background: var(--primary); color: var(--primary-foreground); }
.rc-hint { opacity: .6; font-size: 10px; margin-left: auto; flex-shrink: 0; }
.rc-opt.sel .rc-hint { opacity: .8; }
.rc-warn { color: var(--destructive); opacity: .75; }
.rc-opt.sel .rc-warn { color: var(--primary-foreground); }
.rc-divider { height: 1px; background: var(--border); margin: 4px 2px; opacity: .7; }
.rc-manage { margin-top: 3px; font-size: 11px; opacity: .85; }
.rc-advisor {
  display: flex; align-items: center; gap: 6px; font-size: 11px;
  color: var(--muted-foreground); padding: 5px 2px 1px;
  border-top: 1px solid var(--border); margin-top: 5px;
}
.rc-foot { font-size: 9px; color: var(--muted-foreground); opacity: .75; padding: 4px 2px 0; line-height: 1.4; }
</style>
