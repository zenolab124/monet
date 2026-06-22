<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useWorkbench, setRightZoneWidth } from '@/composables/useWorkbench'
import { useRaceInput } from '@/composables/useRaceInput'
import WorkbenchColumnView from './WorkbenchColumn.vue'

const { activeTab } = useWorkbench()
const { t } = useI18n()

const race = computed(() => activeTab.value.race!)

const {
  inputText,
  textareaRef,
  imageInput,
  slashError,
  anyStreaming,
  streamingCount,
  broadcastSend,
  stopAll,
  forkNewLane,
} = useRaceInput(activeTab)

const containerRef = ref<HTMLElement>()

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  const el = containerRef.value
  if (!el) return
  setRightZoneWidth(el.clientWidth)
  resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      if (entry.contentRect.width > 0) setRightZoneWidth(entry.contentRect.width)
    }
  })
  resizeObserver.observe(el)
  imageInput.attach()
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
})

function autoResize() {
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 160) + 'px'
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    broadcastSend()
  }
}
</script>

<template>
  <div class="flex-1 min-w-0 h-full flex flex-col">
    <!-- 列区 -->
    <div ref="containerRef" class="flex-1 min-h-0 flex flex-row p-2.5 gap-2.5">
      <div
        v-for="(col, i) in activeTab.columns"
        :key="col.id"
        class="flex-1 min-w-0 relative"
      >
        <WorkbenchColumnView :column="col" :tab-id="activeTab.id" :index="i" />
      </div>

      <!-- 添加赛道 -->
      <button
        class="shrink-0 w-8 self-stretch grid place-items-center rounded border border-dashed border-border text-muted-foreground hover:text-foreground hover:border-ring transition-colors"
        :title="t('workbench.race.addLane')"
        @click="forkNewLane"
      >
        <span class="i-carbon-add w-4 h-4" />
      </button>
    </div>

    <!-- 共享输入区 -->
    <div class="px-4 py-3 border-t border-border shrink-0 relative">
      <div v-if="slashError" class="mb-1 text-xs text-destructive">
        {{ slashError }}
      </div>

      <div v-if="imageInput.images.value.length" class="mb-2 flex gap-2 flex-wrap">
        <div v-for="img in imageInput.images.value" :key="img.id" class="relative w-14 h-14 rounded border border-border overflow-hidden group">
          <img :src="img.dataUrl" class="w-full h-full object-cover" />
          <button
            class="absolute top-0 right-0 w-4 h-4 rounded-bl bg-destructive/80 text-destructive-foreground flex items-center justify-center text-2.5 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
            @click="imageInput.removeImage(img.id)"
          >&times;</button>
        </div>
      </div>

      <div v-if="imageInput.lastError.value" class="mb-1 text-xs text-destructive">
        {{ imageInput.lastError.value.message }}
      </div>

      <div class="flex items-center gap-2">
        <textarea
          ref="textareaRef"
          v-model="inputText"
          :placeholder="t('workbench.race.sharedInput')"
          rows="1"
          class="flex-1 px-3 py-2 text-sm rounded-md bg-popover border border-border text-foreground placeholder-muted-foreground resize-none focus:outline-none focus:border-ring transition-colors"
          @keydown="onInputKeydown"
          @input="autoResize"
        />

        <button
          v-if="anyStreaming && !inputText.trim() && !imageInput.images.value.length"
          class="px-3 py-2 text-xs rounded-md bg-accent text-accent-foreground hover:shadow-paper transition-shadow shrink-0"
          @click="stopAll"
        >
          {{ t('workbench.race.stopAll') }}
          <span v-if="streamingCount > 0" class="ml-1 opacity-60">{{ streamingCount }}/{{ race.lanes.length }}</span>
        </button>

        <button
          v-else
          :disabled="!inputText.trim() && !imageInput.images.value.length"
          class="px-3 py-2 text-xs rounded-md bg-primary text-primary-foreground hover:shadow-paper transition-shadow shrink-0 disabled:opacity-30 disabled:cursor-not-allowed"
          @click="broadcastSend"
        >
          {{ t('common.send') }}
        </button>
      </div>
    </div>
  </div>
</template>
