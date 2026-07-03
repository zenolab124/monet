<script setup lang="ts">
const props = defineProps<{
  /** args:parsePrivateTags 已把紧邻的 command-args 合并挂到 command-name 块上(单行渲染) */
  block: { type: 'command-name' | 'command-args' | 'local-command-stdout'; text: string; args?: string; [key: string]: unknown }
}>()
</script>

<template>
  <!-- command-name: /xxx [args] 单行 -->
  <div v-if="props.block.type === 'command-name'" class="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground min-w-0">
    <span class="i-carbon-terminal w-3 h-3 shrink-0" />
    <code class="bg-muted px-1 py-0.5 rounded text-xs shrink-0">{{ props.block.text }}</code>
    <span v-if="props.block.args" class="font-mono text-muted-foreground/70 truncate">{{ props.block.args }}</span>
  </div>
  <!-- command-args: 参数(未被合并的孤立块,兜底保留) -->
  <div v-else-if="props.block.type === 'command-args' && props.block.text" class="mt-0.5 pl-5 text-xs text-muted-foreground/70">
    {{ props.block.text }}
  </div>
  <!-- local-command-stdout: 执行结果 -->
  <div v-else-if="props.block.type === 'local-command-stdout' && props.block.text" class="mt-0.5 pl-5 text-xs text-muted-foreground/70 flex items-center gap-1">
    <span class="i-carbon-arrow-right w-2.5 h-2.5 shrink-0" />
    {{ props.block.text }}
  </div>
</template>
