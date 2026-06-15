<script setup lang="ts">
defineProps<{
  input: Record<string, unknown>
  toolUseId: string
  name: string
}>()
</script>

<template>
  <div class="mt-2 rounded-md bg-background border border-border px-3 py-2 text-xs">
    <div class="flex items-center gap-1.5">
      <span class="i-carbon-plan w-3.5 h-3.5 shrink-0" />
      <span class="text-foreground font-medium">EnterPlanMode</span>
      <span class="px-1.5 py-0.5 rounded border border-border text-muted-foreground">{{ $t('block.toolPlan.enterPlan') }}</span>
    </div>
  </div>
</template>
