<script setup lang="ts">
/**
 * 斜杠命令帮助卡片（FR-004 的 /help 渲染目标）
 *
 * 作为对话流中的"前端工具结果"渲染：
 *  - 视觉与现有 tool_use 卡片同风格（灰边框、紧凑桌面感）
 *  - 列出本版支持的所有命令、参数提示、说明
 *  - 不调 CLI，纯前端展示
 */
import type { SlashCommand } from '@/composables/useSlashCommands'

defineProps<{
  /** 通常传 SLASH_COMMANDS，由父组件透传 */
  commands: SlashCommand[]
}>()
</script>

<template>
  <div class="mt-2 rounded-md border border-border bg-popover/40 px-3 py-2">
    <!-- 头部 -->
    <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
      <span class="i-carbon-help w-3.5 h-3.5 shrink-0" />
      <span>{{ $t('slash.title') }}</span>
      <span class="text-muted-foreground font-normal">{{ $t('slash.totalCount', { count: commands.length }) }}</span>
    </div>

    <!-- 命令列表 -->
    <ul class="mt-2 space-y-1">
      <li
        v-for="cmd in commands"
        :key="cmd.name"
        class="flex items-baseline gap-2 text-xs"
      >
        <span class="font-mono text-primary shrink-0">/{{ cmd.name }}</span>
        <span
          v-if="cmd.hasArg && cmd.argHint"
          class="font-mono text-muted-foreground shrink-0"
        >
          {{ cmd.argHint }}
        </span>
        <span class="text-muted-foreground break-words">{{ cmd.hint }}</span>
        <span
          v-if="cmd.category === 'pass'"
          class="ml-auto px-1.5 py-0.5 rounded text-2xs text-muted-foreground border border-border shrink-0"
          :title="$t('slash.passThroughTitle')"
        >
          {{ $t('slash.passThrough') }}
        </span>
        <span
          v-else
          class="ml-auto px-1.5 py-0.5 rounded text-2xs text-muted-foreground border border-border shrink-0"
          :title="$t('slash.nativeTitle')"
        >
          {{ $t('slash.native') }}
        </span>
      </li>
    </ul>

    <!-- 底部说明 -->
    <div class="mt-2 pt-2 border-t border-border text-2xs text-muted-foreground">
      {{ $t('slash.footer') }}
    </div>
  </div>
</template>

<style scoped>
.text-2xs {
  font-size: 10px;
  line-height: 1.3;
}
</style>
