<script setup lang="ts">
import { computed, watch } from 'vue'
import DiagnosisCard from '../components/home/DiagnosisCard.vue'
import HeatmapCard from '../components/home/HeatmapCard.vue'
import TokenCard from '../components/home/TokenCard.vue'
import { useHomeStats } from '../composables/useHomeStats'
import { useUiState } from '../composables/useUiState'

/**
 * 首页总览（v2.2.0 实化）：Token 消耗 / 兼容性诊断 / 活跃热力三卡实数据。
 * 首次进入首页惰性触发加载（含启动即恢复到首页的场景），会话期缓存，头部手动刷新。
 */

const { activeSection } = useUiState()
const {
  usage, usageLoading, usageError, retryUsage,
  diag, diagLoading, diagError, diagAt, retryDiag,
  ensureLoaded, refresh,
} = useHomeStats()

watch(
  activeSection,
  (section) => {
    if (section === 'home') ensureLoaded()
  },
  { immediate: true },
)

const refreshing = computed(() => usageLoading.value || diagLoading.value)

const headDate = computed(() => {
  // 显式依赖加载状态：无依赖的 computed 会被永久缓存，
  // 应用跨午夜长驻后点刷新日期不会更新（FR-006 验收第 3 条）
  void refreshing.value
  const d = new Date()
  return `${d.getFullYear()} 年 ${d.getMonth() + 1} 月 ${d.getDate()} 日 · 本月数据`
})
</script>

<template>
  <main class="h-full overflow-y-auto px-8 py-6.5" data-tauri-drag-region>
    <div class="max-w-220 mx-auto">
      <div class="flex items-baseline gap-3 mb-4.5">
        <h1 class="text-lg font-semibold">总览</h1>
        <span class="text-xs text-muted-foreground">{{ headDate }}</span>
        <button
          class="refresh-btn ml-auto"
          :disabled="refreshing"
          title="重新统计"
          @click="refresh"
        >
          <span class="i-carbon-renew w-3.5 h-3.5" :class="{ 'animate-spin': refreshing }" />
        </button>
      </div>

      <div class="card-grid">
        <TokenCard :usage="usage" :loading="usageLoading" :error="usageError" @retry="retryUsage" />
        <DiagnosisCard
          :diag="diag"
          :loading="diagLoading"
          :error="diagError"
          :scanned-at="diagAt"
          @retry="retryDiag"
        />
        <HeatmapCard :usage="usage" :loading="usageLoading" :error="usageError" @retry="retryUsage" />
      </div>
    </div>
  </main>
</template>

<style scoped>
.card-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
/* 窄窗降单列（FR-007 验收口径 <900px） */
@media (max-width: 900px) {
  .card-grid {
    grid-template-columns: 1fr;
  }
}

.refresh-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--muted-foreground);
  background: transparent;
  cursor: pointer;
}
.refresh-btn:hover:not(:disabled) {
  background: var(--muted);
  color: var(--foreground);
}
.refresh-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
