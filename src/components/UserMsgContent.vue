<script setup lang="ts">
import { ref, computed, watch, inject } from 'vue'
import type { ContentBlock } from '@/types'
import { IMAGE_LOCATOR, buildCcimgUrl } from '@/utils/ccimg'
import MessageBlock from './MessageBlock.vue'

const props = defineProps<{
  blocks: ContentBlock[]
  /** 图片所属 record 的 uuid;透传给 BlockImage 拼协议 URL(历史区)。pending 区无此值,走 dataUrl */
  recordUuid?: string | null
}>()

// 会话级定位上下文(单图宽高比探测走协议 URL 时用)
const locator = inject(IMAGE_LOCATOR, null)

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
  // 双路径取探测源:内存 data → dataUrl;已落盘 → ccimg 协议 URL
  let probeUrl: string | null = null
  if (src?.data) {
    probeUrl = `data:${src.media_type};base64,${src.data}`
  } else if (locator?.value && props.recordUuid) {
    probeUrl = buildCcimgUrl(locator.value, props.recordUuid, src?.img_index ?? 0)
  }
  if (!probeUrl) { layout.value = 'col'; return }
  const el = new Image()
  el.onload = () => { layout.value = el.naturalHeight > el.naturalWidth ? 'row' : 'col' }
  el.onerror = () => { layout.value = 'col' }
  el.src = probeUrl
}, { immediate: true })
</script>

<template>
  <!-- 纯文字 -->
  <template v-if="layout === 'text-only'">
    <MessageBlock v-for="(b, i) in blocks" :key="i" :block="b" :record-uuid="recordUuid" />
  </template>

  <!-- 竖图：文左图右 -->
  <div v-else-if="layout === 'row'" class="flex gap-3 items-start">
    <div class="flex-1 min-w-0">
      <MessageBlock v-for="(b, i) in otherBlocks" :key="i" :block="b" :record-uuid="recordUuid" />
    </div>
    <div class="shrink-0 max-w-2/5">
      <MessageBlock v-for="(b, i) in imageBlocks" :key="`img-${i}`" :block="b" :record-uuid="recordUuid" />
    </div>
  </div>

  <!-- 横图 / 多图：图上文下 -->
  <div v-else>
    <div class="user-msg-images mb-1">
      <MessageBlock v-for="(b, i) in imageBlocks" :key="`img-${i}`" :block="b" :record-uuid="recordUuid" />
    </div>
    <MessageBlock v-for="(b, i) in otherBlocks" :key="i" :block="b" :record-uuid="recordUuid" />
  </div>
</template>

<style scoped>
.user-msg-images :deep(.block-image-thumb) {
  max-height: 130px;
}
</style>
