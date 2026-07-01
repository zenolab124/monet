<script setup lang="ts">
/**
 * 输入框上方的图片缩略图区(FR-005)
 *
 * 行为:
 *  - 接收 PendingImage[],grid 布局展示缩略图
 *  - 转发删除事件到父组件
 *  - 数量为 0 时整体不渲染(由父组件用 v-if 控制亦可)
 *
 * 视觉:
 *  - 缩略图横向排列,gap 8px
 *  - 末尾显示"X/5"计数(灰色)
 *  - 与输入框共享 px-4 边距,垂直 mt-2 紧贴文本框
 */
import InputImageThumbnail from './InputImageThumbnail.vue'
import type { PendingImage } from '@/composables/useImageInput'

defineProps<{
  /** 暂存的图片队列 */
  images: PendingImage[]
}>()

const emit = defineEmits<{
  remove: [id: string]
}>()
</script>

<template>
  <div
    v-if="images.length > 0"
    class="flex flex-wrap items-center gap-2 mb-2"
  >
    <InputImageThumbnail
      v-for="img in images"
      :key="img.id"
      :data-url="img.dataUrl"
      :size="img.size"
      :mime="img.mime"
      @remove="emit('remove', img.id)"
    />
    <span v-if="images.length > 1" class="text-2xs text-muted-foreground ml-1">
      {{ images.length }}
    </span>
  </div>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.2;
}
</style>
