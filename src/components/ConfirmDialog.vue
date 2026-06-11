<script setup lang="ts">
import { useConfirm } from '@/composables/useConfirm'

const { visible, message, confirmLabel, settle } = useConfirm()
</script>

<template>
  <!-- 全局确认弹窗:Paper 风格,棕偏遮罩 -->
  <div
    v-if="visible"
    class="fixed inset-0 z-70 grid place-items-center"
    style="background: rgba(70, 45, 20, 0.18)"
    @mousedown.self="settle(false)"
  >
    <div class="w-80 rounded bg-popover border border-border shadow-paper-lifted p-4">
      <p class="text-sm text-foreground whitespace-pre-wrap">{{ message }}</p>
      <div class="mt-4 flex justify-end gap-2">
        <button
          class="px-3 py-1.5 text-xs rounded-md border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
          @click="settle(false)"
        >
          取消
        </button>
        <button
          class="px-3 py-1.5 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow"
          @click="settle(true)"
        >
          {{ confirmLabel }}
        </button>
      </div>
    </div>
  </div>
</template>
