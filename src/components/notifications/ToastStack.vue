<script setup lang="ts">
import { computed } from 'vue'
import { useNotifications, requestKindLabel, type PersistentToast } from '@/composables/useNotifications'
import { usePermissionRequests, isInteractiveTool } from '@/composables/usePermissionRequests'

/**
 * 右下角通知层(FR-006):任何域可见。
 * 持久型(权限请求/出错停住)处理前不消失,同屏上限 3 条,超出折叠为汇总条;
 * 瞬态型 5 秒自动消失。与左列决策条同源(权限队列/错误事件集),任一处处理同步消失。
 */
const { transients, persistentToasts, toastsExpanded, dismissError, goToSession, retryFromToast } = useNotifications()
const { respondRequest } = usePermissionRequests()

const PERSISTENT_LIMIT = 3

const visiblePersistent = computed<PersistentToast[]>(() =>
  toastsExpanded.value ? persistentToasts.value : persistentToasts.value.slice(0, PERSISTENT_LIMIT),
)

const foldedCount = computed(() =>
  Math.max(0, persistentToasts.value.length - PERSISTENT_LIMIT),
)

async function onAllow(t: PersistentToast) {
  if (t.kind === 'permission') await respondRequest(t.request.requestId, 'allow_once')
}

async function onDeny(t: PersistentToast) {
  if (t.kind === 'permission') await respondRequest(t.request.requestId, 'deny')
}

/** 交互工具(提问/计划)不提供就地允许/拒绝——必须去会话里作答 */
function needsSession(t: PersistentToast): boolean {
  return t.kind === 'permission' && isInteractiveTool(t.request.toolName)
}

function toastLabel(t: PersistentToast): string {
  return t.kind === 'permission' ? requestKindLabel(t.request.toolName) : '出错停住'
}

function toastIcon(t: PersistentToast): string {
  if (t.kind === 'error') return 'i-carbon-warning text-destructive'
  switch (t.request.toolName) {
    case 'AskUserQuestion': return 'i-carbon-help text-accent'
    case 'ExitPlanMode':
    case 'EnterPlanMode': return 'i-carbon-task-approved text-accent'
    default: return 'i-carbon-locked text-accent'
  }
}
</script>

<template>
  <div class="fixed right-4 top-[20vh] z-60 w-[340px] flex flex-col gap-2 pointer-events-none">
    <!-- 瞬态型(5s 自动消失) -->
    <TransitionGroup name="toast-fade">
      <div
        v-for="t in transients"
        :key="`tr-${t.id}`"
        class="pointer-events-auto bg-popover border border-border rounded shadow-paper-lifted px-3 py-2"
      >
        <div class="flex items-center gap-1.5 text-xs font-semibold">
          <span class="i-carbon-checkmark-outline w-3.5 h-3.5 text-primary shrink-0" />
          <span class="truncate">{{ t.title }}</span>
        </div>
        <div v-if="t.sub" class="mt-0.5 text-[11px] font-mono text-muted-foreground truncate">{{ t.sub }}</div>
      </div>
    </TransitionGroup>

    <!-- 持久型(处理前不消失) -->
    <TransitionGroup name="toast-fade">
      <div
        v-for="t in visiblePersistent"
        :key="t.key"
        class="pointer-events-auto bg-popover border border-border rounded shadow-paper-lifted px-3 py-2"
        :class="t.kind === 'error' ? 'toast-edge-destructive' : 'toast-edge-accent'"
      >
        <!-- 标题行 = 事件类型 + 会话标题截断 -->
        <div class="flex items-center gap-1.5 text-xs font-semibold">
          <span class="w-3.5 h-3.5 shrink-0" :class="toastIcon(t)" />
          <span class="truncate">{{ toastLabel(t) }} · {{ t.title }}</span>
          <button
            v-if="t.kind === 'error'"
            class="ml-auto shrink-0 text-muted-foreground hover:text-foreground"
            title="忽略"
            @click="dismissError(t.sessionId)"
          >
            <span class="i-carbon-close w-3.5 h-3.5 block" />
          </button>
        </div>

        <!-- 等宽摘要副行 -->
        <div class="mt-0.5 mb-1.5 text-[11px] font-mono text-muted-foreground truncate">{{ t.sub }}</div>

        <!-- 操作 -->
        <div class="flex items-center gap-1.5">
          <template v-if="t.kind === 'permission'">
            <!-- 提问/计划类:就地无法作答,「去会话」提为主操作 -->
            <template v-if="needsSession(t)">
              <button class="px-2.5 py-0.5 text-[11px] rounded bg-primary text-primary-foreground" @click="goToSession(t.sessionId)">去会话回答</button>
            </template>
            <template v-else>
              <button class="px-2.5 py-0.5 text-[11px] rounded bg-primary text-primary-foreground" @click="onAllow(t)">允许</button>
              <button class="px-2.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground" @click="onDeny(t)">拒绝</button>
              <button class="ml-auto px-2.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground" @click="goToSession(t.sessionId)">去会话</button>
            </template>
          </template>
          <template v-else>
            <button
              v-if="t.canRetry"
              class="px-2.5 py-0.5 text-[11px] rounded bg-primary text-primary-foreground"
              @click="retryFromToast(t.sessionId)"
            >重试</button>
            <button class="px-2.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground" @click="goToSession(t.sessionId)">去会话</button>
          </template>
        </div>
      </div>
    </TransitionGroup>

    <!-- 折叠汇总条 -->
    <button
      v-if="!toastsExpanded && foldedCount > 0"
      class="pointer-events-auto bg-popover border border-border rounded shadow-paper px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground text-left"
      @click="toastsExpanded = true"
    >
      还有 {{ foldedCount }} 件需要处理
    </button>
    <button
      v-else-if="toastsExpanded && persistentToasts.length > PERSISTENT_LIMIT"
      class="pointer-events-auto bg-popover border border-border rounded shadow-paper px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground text-left"
      @click="toastsExpanded = false"
    >
      收起列表
    </button>
  </div>
</template>

<style scoped>
.toast-edge-accent {
  border-left: 3px solid var(--accent);
}
.toast-edge-destructive {
  border-left: 3px solid var(--destructive);
}
.toast-fade-enter-active,
.toast-fade-leave-active {
  transition: opacity 180ms ease, transform 180ms ease;
}
.toast-fade-enter-from,
.toast-fade-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
