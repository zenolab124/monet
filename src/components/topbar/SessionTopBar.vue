<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { shortModel, relativeTime } from '@/types'
import { inferModel, getContextWindow } from '@/utils/modelContext'
import type { EffortLevel } from '@/composables/useSessionSettings'
import ActionBar from '../ActionBar.vue'
import ModelDropdown from './ModelDropdown.vue'
import ContextProgress from './ContextProgress.vue'
import EffortDropdown from './EffortDropdown.vue'

const props = defineProps<{
  /** 已计算好的 displayTitle */
  title: string
  /** 完整 sessionId */
  sessionId: string
  /** 已计算好的 shortId */
  shortIdValue: string
  /** projectId(透传给 ActionBar 删除用) */
  projectId: string
  /** session.cwd(透传给 ActionBar) */
  cwd: string | null
  /** git 分支 */
  gitBranch: string | null
  /** 真实跑过的最近 assistant 消息的 model 字符串(jsonl 里 message.model;含 [1m] 后缀如有) */
  modelString: string | null
  /** 已占用上下文 token 数(最近一次 assistant 响应的 input + cache_read_input_tokens) */
  usedContextTokens: number
  /** 最后修改时间(秒级时间戳) */
  lastModified: number
  /** 用户已选择的模型 ID(来自 useSessionSettings,可能为 null) */
  selectedModelId: string | null
  /** 用户已选择的努力等级 */
  selectedEffort: EffortLevel
  /** 是否显示分屏按钮 */
  showSplit?: boolean
}>()

const emit = defineEmits<{
  (e: 'modelChange', modelId: string): void
  (e: 'effortChange', effort: EffortLevel): void
  (e: 'split-right'): void
  (e: 'close'): void
  (e: 'reload'): void
  (e: 'deleted'): void
}>()

// --- 派生数据 ---

/** 用户在顶栏选的模型(用于下拉显示当前选中);未选则用 modelString 真实值 */
const effectiveModelStr = computed(() => props.selectedModelId ?? props.modelString)

/** 解析后的 ModelInfo(用于决定元数据区是否重复展示模型,null 代表"未知") */
const effectiveModel = computed(() => inferModel(effectiveModelStr.value))

/** 上下文容量:按 jsonl 里真实跑过的模型字符串(含 [1m] 后缀)推断 */
const capacity = computed(() => getContextWindow(props.modelString))

// --- 窄分屏折叠 ---
//
// PRD L350:容器 < 480px 时按优先级折叠
// 折叠顺序:努力等级 → token 进度 → 模型切换(模型最后才折叠)
//
// 阈值划分(经验值,基于按钮+文本估算):
//   >= 480px : 全部展示
//   >= 380px : 折叠努力等级
//   >= 280px : 折叠 token 进度
//   <  280px : 仅显示模型

const containerRef = ref<HTMLElement>()
const containerWidth = ref(Number.POSITIVE_INFINITY)

const showEffort = computed(() => containerWidth.value >= 480)
const showProgress = computed(() => containerWidth.value >= 380)
// 模型始终显示

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

// --- 溢出菜单(被折叠的项) ---

const overflowOpen = ref(false)
const overflowRef = ref<HTMLElement>()

const hasOverflow = computed(() => !showEffort.value || !showProgress.value)

function toggleOverflow() {
  overflowOpen.value = !overflowOpen.value
}

function onDocClick(e: MouseEvent) {
  if (!overflowOpen.value) return
  const target = e.target as Node
  if (overflowRef.value && !overflowRef.value.contains(target)) {
    overflowOpen.value = false
  }
}

onMounted(() => document.addEventListener('mousedown', onDocClick))
onUnmounted(() => document.removeEventListener('mousedown', onDocClick))

// --- 事件转发 ---

function onModelChange(id: string) {
  emit('modelChange', id)
}
function onEffortChange(level: EffortLevel) {
  emit('effortChange', level)
}
</script>

<template>
  <div ref="containerRef" class="px-4 py-3 border-b border-divider shrink-0">
    <!-- 上排:标题 + 操作按钮 -->
    <div class="flex items-start justify-between gap-2">
      <h2 class="text-base font-semibold text-default truncate flex-1">
        {{ title }}
      </h2>
      <div class="flex items-center gap-1 shrink-0">
        <template v-if="showSplit">
          <button
            class="p-1 rounded text-default4 hover:text-default3 hover:bg-hover transition-colors"
            title="右侧分屏"
            @click="emit('split-right')"
          >
            <span class="i-carbon-split-screen w-3.5 h-3.5" />
          </button>
          <button
            class="p-1 rounded text-default4 hover:text-red-400 hover:bg-red-500/10 transition-colors"
            title="关闭面板"
            @click="emit('close')"
          >
            <span class="i-carbon-close w-3.5 h-3.5" />
          </button>
        </template>
        <button
          class="px-2 py-1 text-xs rounded-md text-default3 hover:text-default hover:bg-hover transition-colors flex items-center gap-1"
          title="刷新会话"
          @click="emit('reload')"
        >
          <span class="i-carbon-renew w-3.5 h-3.5" />
          刷新
        </button>
        <ActionBar
          :session-id="sessionId"
          :project-id="projectId"
          :cwd="cwd"
          @deleted="emit('deleted')"
        />
      </div>
    </div>

    <!-- 下排:控件 + 元数据 -->
    <div class="mt-2 flex items-center gap-2 flex-wrap">
      <!-- 模型切换(始终显示) -->
      <ModelDropdown
        :current="effectiveModelStr"
        @select="onModelChange"
      />

      <!-- token 上下文进度(中等宽度起显示) -->
      <ContextProgress
        v-if="showProgress"
        :used="usedContextTokens"
        :capacity="capacity"
      />

      <!-- 努力等级(宽屏起显示) -->
      <EffortDropdown
        v-if="showEffort"
        :current="selectedEffort"
        @select="onEffortChange"
      />

      <!-- 溢出菜单 -->
      <div v-if="hasOverflow" ref="overflowRef" class="relative inline-flex">
        <button
          type="button"
          class="px-2 py-1 text-xs rounded-md text-default3 hover:text-default hover:bg-hover
                 transition-colors flex items-center gap-1 border border-divider"
          title="更多控件"
          :aria-expanded="overflowOpen"
          @click="toggleOverflow"
        >
          <span class="i-carbon-overflow-menu-horizontal w-3.5 h-3.5" />
        </button>
        <div
          v-if="overflowOpen"
          class="absolute top-full right-0 mt-1 z-50 p-2 rounded-md border border-divider
                 shadow-lg bg-input flex flex-col gap-2 min-w-56"
        >
          <ContextProgress
            v-if="!showProgress"
            :used="usedContextTokens"
            :capacity="capacity"
          />
          <EffortDropdown
            v-if="!showEffort"
            :current="selectedEffort"
            @select="onEffortChange"
          />
        </div>
      </div>

      <!-- 元数据(右侧) -->
      <div class="text-xs text-default4 flex items-center gap-2 flex-wrap ml-auto">
        <span>ID: {{ shortIdValue }}</span>
        <span v-if="gitBranch">
          · 分支: <span class="text-purple-400">{{ gitBranch }}</span>
        </span>
        <span v-if="modelString && !effectiveModel">
          · 模型: {{ shortModel(modelString) }}
        </span>
        <span>· {{ relativeTime(lastModified) }}</span>
      </div>
    </div>
  </div>
</template>
