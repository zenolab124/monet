<script setup lang="ts">
import { computed } from 'vue'
import { renderMarkdownPlain } from '@/composables/useMarkdown'

// 段级纯投影叶子(PRD v2.5.0 FR-001):
// Vue 对基元 prop 做相等短路——props 不变则本组件不重渲、v-html 不重设,
// 这就是段级 memo 的全部机制。因此本组件禁止本地状态/watch/事件监听,
// 交互(如代码复制)由 BlockText 容器事件委托处理。
const props = defineProps<{
  /** 段原文(markdown) */
  source: string
  /** 上色后 HTML(shiki 完成态),提供时优先于 plain 渲染 */
  colored?: string
}>()

const html = computed(() => props.colored ?? renderMarkdownPlain(props.source))
</script>

<template>
  <div v-html="html" />
</template>
