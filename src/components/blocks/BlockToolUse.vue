<script setup lang="ts">
import type { ContentBlock } from '@/types'
import { resolveTool } from './tools'

defineProps<{
  block: Extract<ContentBlock, { type: 'tool_use' }>
}>()
</script>

<template>
  <component
    :is="resolveTool(block.name)"
    :input="block.input"
    :tool-use-id="block.id"
    :name="block.name"
  />
</template>
