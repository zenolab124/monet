<script setup lang="ts">
import { ref, computed, provide } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SubAgentState, AsyncTask } from '@/composables/useSubAgents'
import type { SubAgentMeta, SessionRecord, ContentBlock } from '@/types'
import { shortModel, formatTokens } from '@/types'
import { filterConsumedResults } from '@/utils/toolPair'
import { IMAGE_LOCATOR, type ImageLocator } from '@/utils/ccimg'
import MessageBlock from './MessageBlock.vue'
import MsgClamp from './MsgClamp.vue'
import UserMsgContent from './UserMsgContent.vue'

const props = defineProps<{
  tasks: AsyncTask[]
  openTabs: SubAgentState[]
  activeTabId: string | null
  projectId?: string | null
  sessionId?: string | null
}>()

const emit = defineEmits<{
  selectAgent: [meta: SubAgentMeta]
  closeTab: [agentId: string]
  close: []
  locate: [toolUseId: string]
}>()

const { t } = useI18n()

const imageLocator = computed<ImageLocator | null>(() => {
  if (!props.projectId || !props.sessionId || !props.activeTabId) return null
  return { projectId: props.projectId, sessionId: props.sessionId, agentId: props.activeTabId }
})
provide(IMAGE_LOCATOR, imageLocator)

const activeAgent = computed(() =>
  props.openTabs.find(a => a.meta.agent_id === props.activeTabId) ?? null,
)

const view = ref<'list' | 'detail'>('list')

function onSelectAgent(meta: SubAgentMeta) {
  emit('selectAgent', meta)
  view.value = 'detail'
}

function onSelectTask(task: AsyncTask) {
  if (task.type === 'agent') {
    const meta: SubAgentMeta = {
      agent_id: task.id,
      tool_use_id: task.toolUseId,
      agent_type: task.label,
      description: task.description,
    }
    onSelectAgent(meta)
  }
}

function backToList() {
  view.value = 'list'
}

interface MessageGroup {
  user: SessionRecord | null
  responses: SessionRecord[]
}

function contentBlocks(record: Extract<SessionRecord, { type: 'user' | 'assistant' }>): ContentBlock[] {
  if (!record.message) return []
  if (record.type === 'user') {
    const content = record.message.content
    return typeof content === 'string'
      ? [{ type: 'text', text: content }]
      : content
  }
  return filterConsumedResults(record.message.content)
}

function groupMessages(records: SessionRecord[]): MessageGroup[] {
  const groups: MessageGroup[] = []
  let current: MessageGroup | null = null
  for (const record of records) {
    if (record.type === 'user') {
      current = { user: record, responses: [] }
      groups.push(current)
    } else if (record.type === 'assistant' || record.type === 'system') {
      if (!current) {
        current = { user: null, responses: [] }
        groups.push(current)
      }
      current.responses.push(record)
    }
  }
  return groups
}

const messageGroups = computed<MessageGroup[]>(() =>
  activeAgent.value ? groupMessages(activeAgent.value.records) : [],
)
</script>

<template>
  <div class="async-panel-root h-full flex flex-col bg-card">
    <!-- Header -->
    <div class="shrink-0 h-10 flex items-center px-3 border-b border-border gap-2">
      <button
        v-if="view === 'detail'"
        class="w-5 h-5 flex items-center justify-center rounded hover:bg-muted transition-colors"
        @click="backToList"
      >
        <span class="i-carbon-chevron-left w-3.5 h-3.5" />
      </button>
      <span v-if="view === 'list'" class="i-carbon-lightning w-3.5 h-3.5 text-claude shrink-0" />
      <span class="text-xs font-semibold flex-1 truncate">
        <template v-if="view === 'list'">{{ t('asyncTask.title') }}</template>
        <template v-else>{{ activeAgent?.meta.description || activeAgent?.meta.agent_type || 'Agent' }}</template>
      </span>
      <span
        v-if="view === 'list'"
        class="px-1.5 py-0.5 rounded-full text-[10px] font-semibold bg-claude/15 text-claude tabular-nums"
      >{{ tasks.length }}</span>
      <button
        class="w-5 h-5 flex items-center justify-center rounded hover:bg-muted transition-colors"
        :title="t('common.close')"
        @click="emit('close')"
      >
        <span class="i-carbon-close w-3 h-3" />
      </button>
    </div>

    <!-- List View -->
    <div v-if="view === 'list'" class="flex-1 min-h-0 overflow-y-auto p-2 space-y-1.5 overscroll-contain">
      <div
        v-for="task in tasks"
        :key="task.id"
        class="overflow-hidden"
      >
        <div
          class="px-2.5 py-2 rounded text-[11px]
                 bg-background border border-border cursor-pointer
                 transition-colors hover:bg-card hover:border-primary/40"
          @click="onSelectTask(task)"
        >
          <div class="flex items-center gap-1.5 mb-1">
            <span
              class="px-1 py-0.5 rounded text-[9px] font-semibold shrink-0"
              :class="task.type === 'workflow'
                ? 'bg-claude/15 text-claude'
                : 'bg-tag text-tag-foreground'"
            >{{ task.type === 'workflow' ? 'Workflow' : task.label === 'general-purpose' ? 'Agent' : task.label }}</span>
            <span class="flex-1" />
            <button
              v-if="task.toolUseId"
              class="w-4 h-4 flex items-center justify-center rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
              :title="t('asyncTask.locate')"
              @click.stop="emit('locate', task.toolUseId)"
            >
              <span class="i-carbon-location w-3 h-3" />
            </button>
          </div>
          <div class="text-foreground leading-relaxed">{{ task.description }}</div>
        </div>
        <!-- Workflow 子 Agent 折叠列表 -->
        <div v-if="task.type === 'workflow' && task.children.length" class="ml-6 mt-0.5 space-y-0.5">
          <div
            v-for="child in task.children"
            :key="child.agent_id"
            class="flex gap-1.5 px-2 py-1.5 rounded bg-background text-[10px]
                   text-muted-foreground cursor-pointer hover:text-foreground hover:bg-card transition-colors"
            @click.stop="onSelectAgent(child)"
          >
            <span class="w-1.5 h-1.5 rounded-full bg-primary shrink-0 mt-1" />
            <span class="flex-1 leading-relaxed">{{ child.description || child.agent_type || child.agent_id }}</span>
          </div>
        </div>
      </div>

      <div v-if="tasks.length === 0" class="text-center text-muted-foreground text-xs py-8">
        {{ t('asyncTask.empty') }}
      </div>
    </div>

    <!-- Detail View -->
    <template v-else>
      <div v-if="!activeAgent" class="flex-1 flex items-center justify-center">
        <p class="text-muted-foreground text-xs">{{ t('asyncTask.empty') }}</p>
      </div>

      <div v-else-if="activeAgent.loading" class="flex-1 flex items-center justify-center">
        <p class="text-muted-foreground text-xs">{{ t('session.loadingChat') }}</p>
      </div>

      <div v-else class="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-3 overscroll-contain">
        <div v-for="(group, gi) in messageGroups" :key="gi" class="space-y-3">
          <div v-if="group.user && group.user.type === 'user'">
            <div class="flex gap-2">
              <div class="w-0.5 shrink-0 rounded-full bg-primary/60" />
              <div class="min-w-0 flex-1 bg-background border border-border rounded px-2.5 py-1.5">
                <div class="text-[10px] font-medium mb-0.5 text-primary">Prompt</div>
                <MsgClamp>
                  <UserMsgContent :blocks="contentBlocks(group.user as any)" :record-uuid="(group.user as any).uuid" />
                </MsgClamp>
              </div>
            </div>
          </div>
          <template v-for="resp in group.responses" :key="(resp as any).uuid || resp">
            <div v-if="resp.type === 'assistant'" class="flex gap-2">
              <div class="w-0.5 shrink-0 rounded-full bg-claude/60" />
              <div class="min-w-0 flex-1">
                <div class="text-[10px] font-medium mb-0.5 text-claude flex items-center gap-1">
                  <span>
                    Agent
                    <span v-if="(resp as any).message?.model" class="text-muted-foreground font-normal">
                      ({{ shortModel((resp as any).message.model) }})
                    </span>
                  </span>
                  <span v-if="(resp as any).message?.usage" class="text-muted-foreground/70 font-normal tabular-nums">
                    {{ formatTokens((resp as any).message.usage.input_tokens) }} in
                    · {{ formatTokens((resp as any).message.usage.output_tokens) }} out
                  </span>
                </div>
                <div>
                  <MessageBlock
                    v-for="(block, bi) in contentBlocks(resp as any)"
                    :key="bi"
                    :block="block"
                    :record-uuid="(resp as any).uuid"
                  />
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>
    </template>
  </div>
</template>
