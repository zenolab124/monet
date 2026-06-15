<script setup lang="ts">
import MonitorRail from '@/components/workbench/MonitorRail.vue'
import WorkbenchColumns from '@/components/workbench/WorkbenchColumns.vue'
import { useWorkbench } from '@/composables/useWorkbench'

const { activeTab } = useWorkbench()
</script>

<template>
  <div class="h-full flex min-h-0">
    <MonitorRail />
    <WorkbenchColumns :key="activeTab.id" />
  </div>
</template>
