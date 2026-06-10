<script setup lang="ts">
/**
 * 权限请求卡片(FR-003)
 *
 * 显示单条权限请求,提供三档决策:
 *   - 允许一次  (主按钮,蓝,默认聚焦)
 *   - 允许此会话(警示按钮,橙)
 *   - 拒绝     (次按钮,灰)
 *
 * 头部包含工具名、危险标识、剩余倒计时。
 * 中部复用 blocks/tools/ 的工具组件做参数预览(同一套视觉)。
 *
 * 键盘:
 *   - Enter        → allow_once(默认按钮)
 *   - Esc          → deny
 *   - Tab/Shift+Tab → 在三按钮间循环
 *
 * 由父组件控制是否挂载(基于 usePermissionRequests().current)。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { resolveTool } from './blocks/tools'
import type { PermissionRequest, PermissionDecision } from '@/composables/usePermissionRequests'

const props = defineProps<{
  request: PermissionRequest
}>()

const emit = defineEmits<{
  (e: 'decide', decision: PermissionDecision): void
}>()

// --- 倒计时 ---

/** 当前时间戳(每秒刷一次) */
const now = ref(Date.now())
let timer: number | null = null

onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now()
    // 超时自动拒绝(防御:Rust 端也会超时,这里做一次 UI 兜底)
    if (now.value >= props.request.timeoutAt) {
      emit('decide', 'deny')
    }
  }, 1000)
  // 默认聚焦"允许一次"
  allowOnceBtn.value?.focus()
  // 全局键盘监听(Enter/Esc)
  window.addEventListener('keydown', onKeydown, { capture: true })
})

onBeforeUnmount(() => {
  if (timer !== null) {
    clearInterval(timer)
    timer = null
  }
  window.removeEventListener('keydown', onKeydown, { capture: true } as any)
})

const remainingSec = computed(() => {
  const diff = Math.max(0, props.request.timeoutAt - now.value)
  return Math.ceil(diff / 1000)
})

const remainingPct = computed(() => {
  const total = 60_000
  const remain = Math.max(0, props.request.timeoutAt - now.value)
  return Math.min(100, Math.max(0, (remain / total) * 100))
})

// --- 工具组件解析 ---

const ToolComponent = computed(() => resolveTool(props.request.toolName))

const isDanger = computed(() => props.request.danger !== null)

// --- 按钮 refs ---

const allowOnceBtn = ref<HTMLButtonElement | null>(null)
const allowSessionBtn = ref<HTMLButtonElement | null>(null)
const denyBtn = ref<HTMLButtonElement | null>(null)

// --- 决策 ---

function decide(decision: PermissionDecision) {
  emit('decide', decision)
}

// --- 全局键盘 ---

function onKeydown(e: KeyboardEvent) {
  // Enter:仅当焦点在卡片内任一按钮才生效;不在卡片内时让默认按钮处理
  if (e.key === 'Enter') {
    const target = e.target as HTMLElement | null
    // 当焦点在三按钮之一,各按钮自身的 click 事件会处理 Enter,不必拦截
    // 焦点不在按钮但卡片可见时,默认走 allow_once
    if (
      target !== allowOnceBtn.value &&
      target !== allowSessionBtn.value &&
      target !== denyBtn.value
    ) {
      e.preventDefault()
      e.stopPropagation()
      decide('allow_once')
    }
    return
  }
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    decide('deny')
    return
  }
}
</script>

<template>
  <div
    class="permission-card rounded-md border bg-popover shadow-paper-lifted"
    :class="isDanger ? 'border-accent/60 ring-1 ring-accent/25' : 'border-border'"
    role="alertdialog"
    :aria-label="isDanger ? '高风险权限请求' : '权限请求'"
  >
    <!-- 头部 -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-border">
      <span
        v-if="isDanger"
        class="i-carbon-warning-alt w-4 h-4 text-accent shrink-0"
        aria-hidden="true"
      />
      <span v-else class="i-carbon-locked w-4 h-4 text-muted-foreground shrink-0" aria-hidden="true" />

      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-1.5 flex-wrap">
          <span class="text-xs text-muted-foreground">权限请求</span>
          <span class="text-sm font-medium text-foreground truncate" :title="request.toolName">
            {{ request.toolName }}
          </span>
          <span
            v-if="isDanger"
            class="px-1.5 py-0.5 rounded border border-accent/50 text-accent text-2xs font-medium shrink-0"
          >
            高风险操作
          </span>
        </div>
        <div
          v-if="isDanger && request.danger"
          class="text-2xs text-accent/90 mt-0.5 truncate"
          :title="request.danger.reason"
        >
          {{ request.danger.reason }}
        </div>
      </div>

      <!-- 倒计时 -->
      <div class="text-2xs text-muted-foreground font-mono shrink-0 tabular-nums" aria-live="polite">
        剩余 {{ remainingSec }}s
      </div>
    </div>

    <!-- 倒计时进度条(发条) -->
    <div class="h-0.5 bg-muted">
      <div
        class="h-full transition-all"
        :class="isDanger ? 'bg-accent/60' : 'bg-primary/60'"
        :style="{ width: `${remainingPct}%` }"
      />
    </div>

    <!-- 中部:工具参数预览(复用 blocks/tools 组件) -->
    <div class="px-3 pt-1 pb-2 max-h-72 overflow-y-auto">
      <component
        :is="ToolComponent"
        :input="request.input"
        :tool-use-id="request.requestId"
        :name="request.toolName"
      />
    </div>

    <!-- 底部按钮组 -->
    <div class="flex items-center gap-2 px-3 py-2 border-t border-border">
      <button
        ref="allowOnceBtn"
        type="button"
        class="btn btn-primary"
        @click="decide('allow_once')"
      >
        <span class="i-carbon-checkmark w-3.5 h-3.5" aria-hidden="true" />
        允许一次
      </button>

      <button
        ref="allowSessionBtn"
        type="button"
        class="btn btn-warn"
        title="本会话内同一工具+同一关键参数自动放行,直到流式结束或会话切换"
        @click="decide('allow_session')"
      >
        <span class="i-carbon-time w-3.5 h-3.5" aria-hidden="true" />
        允许此会话
      </button>

      <div class="flex-1" />

      <button
        ref="denyBtn"
        type="button"
        class="btn btn-ghost"
        @click="decide('deny')"
      >
        <span class="i-carbon-close w-3.5 h-3.5" aria-hidden="true" />
        拒绝
      </button>
    </div>
  </div>
</template>

<style scoped>
.permission-card {
  min-width: 320px;
  max-width: 640px;
  width: 100%;
}

.text-2xs {
  font-size: 10px;
  line-height: 1.3;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: 4px;
  font-size: 12px;
  line-height: 1.4;
  font-weight: 500;
  transition:
    background-color 120ms ease,
    color 120ms ease,
    box-shadow 120ms ease;
  cursor: pointer;
  border: 1px solid transparent;
  outline: none;
}

.btn:focus-visible {
  box-shadow: 0 0 0 2px var(--ring);
}

/* 三档决策按钮，对齐 messages.html 的 perm-btn 三态 */
.btn-primary {
  background-color: var(--primary);
  color: var(--primary-foreground);
}
.btn-primary:hover {
  box-shadow: var(--shadow-paper);
}

.btn-warn {
  border-color: var(--primary);
  color: var(--primary);
}
.btn-warn:hover {
  background-color: var(--secondary);
}

.btn-ghost {
  border-color: var(--border);
  color: var(--muted-foreground);
}
.btn-ghost:hover {
  background-color: var(--muted);
}

@media (prefers-reduced-motion: reduce) {
  .btn,
  .h-full {
    transition: none !important;
  }
}
</style>
