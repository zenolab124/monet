<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@/types'
import { useThinkingExpand } from '@/composables/useThinkingExpand'

const props = defineProps<{
  block: Extract<ContentBlock, { type: 'thinking' }>
}>()

// 展开状态全局联动:点开一个 = 全部展开并记住,所有实例消费同一份状态
const { thinkingExpanded: expanded, toggle } = useThinkingExpand()

/**
 * 三态判断:
 * - 有明文 → 可折叠展示
 * - 无明文但有 signature → redacted(已加密)
 * - 都没有 → 流式中尚未拿到 delta,显示"思考中..."
 */
const hasPlainText = computed(() => props.block.thinking.length > 0)
const isRedacted = computed(() => !hasPlainText.value && !!props.block.signature)
const isThinking = computed(() => !hasPlainText.value && !props.block.signature)
</script>

<template>
  <div class="mt-2">
    <!-- 有明文:可折叠,显示字数 -->
    <template v-if="hasPlainText">
      <button
        class="text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
        @click="toggle"
      >
        <span class="i-carbon-chevron-right w-3 h-3 transition-transform" :class="{ 'rotate-90': expanded }" />
        思考过程（{{ block.thinking.length }} 字）
      </button>
      <div v-if="expanded" class="mt-1 pl-3 border-l-2 border-border text-xs text-muted-foreground whitespace-pre-wrap">
        {{ block.thinking }}
      </div>
    </template>

    <!-- 流式中,signature/明文都尚未到达 -->
    <div v-else-if="isThinking" class="text-xs text-muted-foreground flex items-center gap-1">
      <span class="i-carbon-thinking w-3 h-3 shrink-0 animate-pulse" />
      思考中...
    </div>

    <!-- redacted:仅显示标识,不可展开(无明文可展) -->
    <div v-else-if="isRedacted" class="text-xs text-muted-foreground flex items-center gap-1">
      <span class="i-carbon-locked w-3 h-3 shrink-0" />
      已思考(内容加密,Anthropic 未暴露明文)
    </div>
  </div>
</template>
