<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { shortModel, relativeTime } from '@/types'
import { inferModel, getContextWindow } from '@/utils/modelContext'
import type { EffortLevel } from '@/composables/useSessionSettings'
import { useConfirm } from '@/composables/useConfirm'
import { useNotifications } from '@/composables/useNotifications'
import ModelDropdown from './ModelDropdown.vue'
import ContextProgress from './ContextProgress.vue'
import EffortDropdown from './EffortDropdown.vue'

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
  /** 最后修改时间(秒级时间戳) */
  lastModified: number
  /** 用户已选择的模型 ID(来自 useSessionSettings,可能为 null) */
  selectedModelId: string | null
  /** 用户已选择的努力等级 */
  selectedEffort: EffortLevel
}>()

const emit = defineEmits<{
  (e: 'modelChange', modelId: string): void
  (e: 'effortChange', effort: EffortLevel): void
  (e: 'reload'): void
  (e: 'deleted'): void
}>()

const { confirm } = useConfirm()
const { notifyTransient } = useNotifications()

// --- 派生数据 ---

/** 用户在顶栏选的模型(用于下拉显示当前选中);未选则用 modelString 真实值 */
const effectiveModelStr = computed(() => props.selectedModelId ?? props.modelString)

/** 解析后的 ModelInfo(决定菜单内是否补充展示未知模型字符串) */
const effectiveModel = computed(() => inferModel(effectiveModelStr.value))

/** 上下文容量:按 jsonl 里真实跑过的模型字符串(含 [1m] 后缀)推断 */
const capacity = computed(() => getContextWindow(props.modelString))

// --- 窄列折叠 ---
//
// 单行布局的折叠顺序:努力等级 → token 进度,模型永不折叠。
// 阈值按紧凑形态估算(模型 ~90 + 进度 ~75 + 努力 ~70 + 菜单 ~30 + gap/padding):
//   >= 360px : 全部展示
//   >= 280px : 折叠努力等级
//   <  280px : 仅模型 + 菜单
// 被折叠的控件进 ⋯ 菜单(完整形态)。

const containerRef = ref<HTMLElement>()
const containerWidth = ref(Number.POSITIVE_INFINITY)

const showEffort = computed(() => containerWidth.value >= 360)
const showProgress = computed(() => containerWidth.value >= 280)

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

function onDocClick(e: MouseEvent) {
  if (!menuOpen.value) return
  const target = e.target as Node
  if (menuRef.value && !menuRef.value.contains(target)) {
    menuOpen.value = false
  }
}

onMounted(() => document.addEventListener('mousedown', onDocClick))
onUnmounted(() => document.removeEventListener('mousedown', onDocClick))

// --- 菜单操作 ---

async function copySessionId() {
  try {
    await navigator.clipboard.writeText(props.sessionId)
    notifyTransient('会话 ID 已复制')
  } catch {
    notifyTransient('复制失败', '请检查剪贴板权限')
  }
  menuOpen.value = false
}

function onReload() {
  menuOpen.value = false
  emit('reload')
}

async function openInTerminal() {
  menuOpen.value = false
  if (!props.cwd) return
  try {
    await invoke('resume_in_terminal', { cwd: props.cwd, sessionId: props.sessionId })
  } catch (e) {
    notifyTransient('终端打开失败', String(e))
  }
}

async function openInVscode() {
  menuOpen.value = false
  if (!props.cwd) return
  try {
    await invoke('resume_in_vscode', { cwd: props.cwd })
  } catch (e) {
    notifyTransient('VS Code 打开失败', String(e))
  }
}

async function onDelete() {
  menuOpen.value = false
  const ok = await confirm('删除该会话的全部记录?此操作不可恢复。', '删除')
  if (!ok) return
  try {
    await invoke('delete_session', { projectId: props.projectId, sessionId: props.sessionId })
    emit('deleted')
  } catch (e) {
    notifyTransient('删除失败', String(e))
  }
}

// --- 事件转发 ---

function onModelChange(id: string) {
  emit('modelChange', id)
}
function onEffortChange(level: EffortLevel) {
  emit('effortChange', level)
}
</script>

<template>
  <div
    ref="containerRef"
    class="px-3 py-1.5 border-b border-border shrink-0 flex items-center gap-1.5"
  >
    <!-- 模型切换(永不折叠) -->
    <ModelDropdown
      :current="effectiveModelStr"
      @select="onModelChange"
    />

    <!-- token 进度(紧凑形态:条 + 百分比) -->
    <ContextProgress
      v-if="showProgress"
      :used="usedContextTokens"
      :capacity="capacity"
      compact
    />

    <!-- 努力等级 -->
    <EffortDropdown
      v-if="showEffort"
      :current="selectedEffort"
      @select="onEffortChange"
    />

    <div class="ml-auto" />

    <!-- ⋯ 统一菜单 -->
    <div ref="menuRef" class="relative inline-flex shrink-0">
      <button
        type="button"
        class="p-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        title="会话信息与操作"
        :aria-expanded="menuOpen"
        @click="menuOpen = !menuOpen"
      >
        <span class="i-carbon-overflow-menu-horizontal w-3.5 h-3.5" />
      </button>

      <div
        v-if="menuOpen"
        class="absolute top-full right-0 mt-1 z-50 py-1 rounded-md border border-border
               shadow-paper-lifted bg-popover w-60"
      >
        <!-- 窄列被折叠的控件 -->
        <div v-if="!showProgress || !showEffort" class="px-2 py-1.5 flex flex-col gap-2 border-b border-border">
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

        <!-- 元数据 -->
        <div class="px-3 py-1.5 text-xs text-muted-foreground flex flex-col gap-1 border-b border-border">
          <button
            class="flex items-center gap-1.5 hover:text-foreground transition-colors text-left"
            title="复制完整会话 ID"
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
        </div>

        <!-- 操作 -->
        <div class="py-1 flex flex-col">
          <button class="menu-item" @click="onReload">
            <span class="i-carbon-renew w-3.5 h-3.5" />刷新会话
          </button>
          <button v-if="cwd" class="menu-item" @click="openInTerminal">
            <span class="i-carbon-terminal w-3.5 h-3.5" />在终端打开
          </button>
          <button v-if="cwd" class="menu-item" @click="openInVscode">
            <span class="i-carbon-code w-3.5 h-3.5" />在 VS Code 打开
          </button>
          <button class="menu-item text-destructive hover:bg-destructive/10" @click="onDelete">
            <span class="i-carbon-trash-can w-3.5 h-3.5" />删除会话
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
</style>
