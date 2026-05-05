<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ContentBlock } from '@/types'

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'thinking' }>
}>()

const expanded = ref(false)

/**
 * Anthropic 的 redacted thinking:thinking 字段为空,仅在邻近字段携带 signature(加密签名)。
 * 此时无法展开明文,只展示"已思考(加密)"标识。
 *
 * 兼容老会话:thinking 非空时按字数渲染、可展开看明文。
 */
const hasPlainText = computed(() => props.block.thinking.length > 0)
</script>

<template>
  <div class="mt-2">
    <!-- 有明文:可折叠,显示字数 -->
    <template v-if="hasPlainText">
      <button
        class="text-xs text-default4 hover:text-default3 flex items-center gap-1"
        @click="expanded = !expanded"
      >
        <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
        思考过程（{{ block.thinking.length }} 字）
      </button>
      <div v-if="expanded" class="mt-1 pl-3 border-l-2 border-default4/30 text-xs text-default3 whitespace-pre-wrap">
        {{ block.thinking }}
      </div>
    </template>

    <!-- redacted:仅显示标识,不可展开(无明文可展) -->
    <div v-else class="text-xs text-default4 flex items-center gap-1">
      <span class="i-carbon-locked w-3 h-3 shrink-0" />
      已思考(内容加密,Anthropic 未暴露明文)
    </div>
  </div>
</template>
