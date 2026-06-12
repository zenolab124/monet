<script setup lang="ts">
import { computed } from 'vue'
import { useNotifications, type PersistentToast } from '@/composables/useNotifications'
import { usePermissionRequests } from '@/composables/usePermissionRequests'

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
</script>

<template>
  <div class="fixed right-4 bottom-4 z-60 w-[340px] flex flex-col gap-2 pointer-events-none">
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
          <span
            class="w-3.5 h-3.5 shrink-0"
            :class="t.kind === 'permission'
              ? 'i-carbon-locked text-accent'
              : 'i-carbon-warning text-destructive'"
          />
          <span class="truncate">{{ t.kind === 'permission' ? '权限请求' : '出错停住' }} · {{ t.title }}</span>
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
            <button class="px-2.5 py-0.5 text-[11px] rounded bg-primary text-primary-foreground" @click="onAllow(t)">允许</button>
            <button class="px-2.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground" @click="onDeny(t)">拒绝</button>
            <button class="ml-auto px-2.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground" @click="goToSession(t.sessionId)">去会话</button>
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
  transform: translateY(6px);
}
</style>
