<script setup lang="ts">
import type { ContentBlock } from '@/types'
import { resolveBlock } from './blocks'

defineProps<{
  block: ContentBlock
  /** 会话是否仍在流式中,仅 text 块消费(降级渲染);其余块给 undefined 避免 attr 落到 DOM */
  streaming?: boolean
}>()
</script>

<template>
  <component
    :is="resolveBlock(block)"
    :block="block"
    :streaming="block.type === 'text' ? streaming : undefined"
  />
</template>
