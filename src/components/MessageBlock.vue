<script setup lang="ts">
import type { ContentBlock } from '@/types'
import { resolveBlock } from './blocks'

defineProps<{
  block: ContentBlock
}>()
</script>

<template>
  <component :is="resolveBlock(block)" :block="block" />
</template>
