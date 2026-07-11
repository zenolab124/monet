<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import type { AsyncTaskItem } from '@/composables/useAsyncTasks'
import type { SubAgentMeta } from '@/types'

// 非 agent 物种的任务详情：bash 输出 / monitor 事件 / wakeup 计划 / generic 结果。
// agent/workflow child 的对话流详情由 AsyncTaskPanel 的 agent 视图承担。

const props = defineProps<{
  task: AsyncTaskItem
}>()

const emit = defineEmits<{
  openChild: [meta: SubAgentMeta]
}>()

const { t } = useI18n()

// bash/generic：后台输出文件尾部（CLI 临时区，可能已被系统清理）
const output = ref<string | null>(null)
const outputLoaded = ref(false)

watch(
  () => props.task.outputFile,
  async (path) => {
    output.value = null
    outputLoaded.value = false
    if (!path || props.task.species === 'agent' || props.task.species === 'workflow') return
    try {
      output.value = await invoke<string | null>('read_task_output', { path, maxBytes: 64 * 1024 })
    } catch {
      output.value = null
    }
    outputLoaded.value = true
  },
  { immediate: true },
)

const scheduledTime = computed(() => {
  if (!props.task.scheduledFor) return null
  return new Date(props.task.scheduledFor).toLocaleString()
})
</script>

<template>
  <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-3 overscroll-contain text-[11px]">
    <!-- 命令 / prompt -->
    <div v-if="task.detail">
      <div class="text-[10px] font-medium mb-1 text-muted-foreground">
        {{ task.species === 'wakeup' ? t('asyncTask.wakePrompt') : t('asyncTask.command') }}
      </div>
      <pre class="px-2.5 py-2 rounded bg-background border border-border whitespace-pre-wrap break-all font-mono text-[10px] leading-relaxed">{{ task.detail }}</pre>
    </div>

    <!-- wakeup：预定触发时刻 -->
    <div v-if="scheduledTime">
      <div class="text-[10px] font-medium mb-1 text-muted-foreground">{{ t('asyncTask.wakeAt') }}</div>
      <div class="text-foreground tabular-nums">{{ scheduledTime }}</div>
    </div>

    <!-- monitor：事件快照流 -->
    <div v-if="task.events.length">
      <div class="text-[10px] font-medium mb-1 text-muted-foreground">{{ t('asyncTask.events') }}</div>
      <pre
        v-for="(ev, i) in task.events"
        :key="i"
        class="px-2.5 py-2 mb-1 rounded bg-background border border-border whitespace-pre-wrap break-all font-mono text-[10px] leading-relaxed"
      >{{ ev }}</pre>
    </div>

    <!-- 终态通知的 result 全文 -->
    <div v-if="task.resultText">
      <div class="text-[10px] font-medium mb-1 text-muted-foreground">{{ t('asyncTask.result') }}</div>
      <pre class="px-2.5 py-2 rounded bg-background border border-border whitespace-pre-wrap break-all text-[10px] leading-relaxed max-h-80 overflow-y-auto">{{ task.resultText }}</pre>
    </div>

    <!-- workflow：子 Agent 清单（可下钻对话流） -->
    <div v-if="task.species === 'workflow' && task.children.length">
      <div class="text-[10px] font-medium mb-1 text-muted-foreground">{{ t('asyncTask.agents') }}</div>
      <div
        v-for="child in task.children"
        :key="child.agent_id"
        class="flex gap-1.5 px-2 py-1.5 mb-0.5 rounded bg-background border border-border text-[10px]
               text-muted-foreground cursor-pointer hover:text-foreground hover:border-primary/40 transition-colors"
        @click="emit('openChild', child)"
      >
        <span class="w-1.5 h-1.5 rounded-full bg-primary shrink-0 mt-1" />
        <span class="flex-1 leading-relaxed">{{ child.description || child.agent_type || child.agent_id }}</span>
      </div>
    </div>

    <!-- 后台输出文件尾部 -->
    <div v-if="task.outputFile && task.species !== 'agent' && task.species !== 'workflow'">
      <div class="text-[10px] font-medium mb-1 text-muted-foreground">{{ t('asyncTask.output') }}</div>
      <pre
        v-if="output"
        class="px-2.5 py-2 rounded bg-background border border-border whitespace-pre-wrap break-all font-mono text-[10px] leading-relaxed max-h-80 overflow-y-auto"
      >{{ output }}</pre>
      <div v-else-if="outputLoaded" class="text-muted-foreground/70">{{ t('asyncTask.outputMissing') }}</div>
    </div>
  </div>
</template>
