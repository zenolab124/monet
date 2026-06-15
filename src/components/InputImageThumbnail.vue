<script setup lang="ts">
/**
 * 输入框图片缩略图(FR-005)
 *
 * 视觉:
 *  - 64x64,圆角 4px
 *  - 右上角 × 删除按钮(默认 hover 显示)
 *  - 暗色模式下边框沿用 divider 变量
 */
import { computed } from 'vue'

const props = defineProps<{
  /** data URL,直接给 <img :src> 用 */
  dataUrl: string
  /** 字节数,用于 tooltip */
  size: number
  /** MIME,用于 tooltip */
  mime: string
}>()

const emit = defineEmits<{
  remove: []
}>()

const tooltip = computed(() => {
  const kb = (props.size / 1024).toFixed(1)
  return `${props.mime} · ${kb} KB`
})
</script>

<template>
  <div
    class="relative w-16 h-16 rounded-[4px] overflow-hidden border border-border bg-popover
           group flex-shrink-0"
    :title="tooltip"
  >
    <img
      :src="dataUrl"
      class="w-full h-full object-cover block"
      :alt="$t('image.pending')"
      draggable="false"
    />
    <!-- 删除按钮:hover 显示 -->
    <button
      type="button"
      class="absolute top-0.5 right-0.5 w-4 h-4 rounded-full
             bg-foreground/80 text-card text-2xs leading-none
             flex items-center justify-center
             opacity-0 group-hover:opacity-100 focus:opacity-100
             transition-opacity"
      :title="$t('image.remove')"
      :aria-label="$t('image.removeAria')"
      @click="emit('remove')"
    >
      <span class="i-carbon-close w-3 h-3" />
    </button>
  </div>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1;
}
</style>
