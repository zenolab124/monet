<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import type { ContentBlock } from '@/types'
import MessageBlock from './MessageBlock.vue'

const props = defineProps<{
  blocks: ContentBlock[]
}>()

type Layout = 'text-only' | 'row' | 'col'
const layout = ref<Layout>('col')

const imageBlocks = computed(() =>
  props.blocks.filter(b => b.type === 'image') as Extract<ContentBlock, { type: 'image' }>[],
)
const otherBlocks = computed(() =>
  props.blocks.filter(b => b.type !== 'image'),
)

watch(() => imageBlocks.value, (imgs) => {
  if (!imgs.length) { layout.value = 'text-only'; return }
  if (imgs.length > 1) { layout.value = 'col'; return }
  const src = imgs[0].source
  if (!src?.data) { layout.value = 'col'; return }
  const el = new Image()
  el.onload = () => { layout.value = el.naturalHeight > el.naturalWidth ? 'row' : 'col' }
  el.onerror = () => { layout.value = 'col' }
  el.src = `data:${src.media_type};base64,${src.data}`
}, { immediate: true })
</script>

<template>
  <!-- 纯文字 -->
  <template v-if="layout === 'text-only'">
    <MessageBlock v-for="(b, i) in blocks" :key="i" :block="b" />
  </template>

  <!-- 竖图：文左图右 -->
  <div v-else-if="layout === 'row'" class="flex gap-3 items-start">
    <div class="flex-1 min-w-0">
      <MessageBlock v-for="(b, i) in otherBlocks" :key="i" :block="b" />
    </div>
    <div class="shrink-0 max-w-2/5">
      <MessageBlock v-for="(b, i) in imageBlocks" :key="`img-${i}`" :block="b" />
    </div>
  </div>

  <!-- 横图 / 多图：图上文下 -->
  <div v-else>
    <div class="user-msg-images mb-1">
      <MessageBlock v-for="(b, i) in imageBlocks" :key="`img-${i}`" :block="b" />
    </div>
    <MessageBlock v-for="(b, i) in otherBlocks" :key="i" :block="b" />
  </div>
</template>

<style scoped>
.user-msg-images :deep(.block-image-thumb) {
  max-height: 130px;
}
</style>
