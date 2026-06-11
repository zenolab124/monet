<script setup lang="ts">
import WorkbenchTabs from '@/components/workbench/WorkbenchTabs.vue'
import MonitorRail from '@/components/workbench/MonitorRail.vue'
import WorkbenchColumns from '@/components/workbench/WorkbenchColumns.vue'
import { useWorkbench } from '@/composables/useWorkbench'

/**
 * 工作台域(v2.1.0):所有"活"会话的唯一交互场。
 * tab 条(上下文分组) + 左列监控卡 + 右区多列会话。
 * 仅渲染激活 tab 的列(非激活 tab 零 DOM 零更新,数据照收——NFR-001)。
 */
const { activeTab } = useWorkbench()
</script>

<template>
  <div class="h-full flex flex-col">
    <WorkbenchTabs />
    <div :key="activeTab.id" class="flex-1 flex min-h-0">
      <MonitorRail />
      <WorkbenchColumns />
    </div>
  </div>
</template>
