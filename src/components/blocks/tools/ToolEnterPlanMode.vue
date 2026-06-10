<script setup lang="ts">
defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()
</script>

<template>
  <div class="mt-2 rounded-md bg-green-500/5 border border-green-500/20 px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-plan w-3.5 h-3.5 shrink-0" />
      <span class="text-green-400 font-medium">EnterPlanMode</span>
      <span class="px-1.5 py-0.5 rounded bg-cyan-500/15 text-cyan-400">进入计划模式</span>
    </div>
  </div>
</template>
