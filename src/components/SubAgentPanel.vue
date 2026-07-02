<script setup lang="ts">
import { computed, provide } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SubAgentState } from '@/composables/useSubAgents'
import type { SessionRecord, ContentBlock } from '@/types'
import { shortModel, formatTokens } from '@/types'
import { filterConsumedResults } from '@/utils/toolPair'
import { IMAGE_LOCATOR, type ImageLocator } from '@/utils/ccimg'
import MessageBlock from './MessageBlock.vue'
import MsgClamp from './MsgClamp.vue'
import UserMsgContent from './UserMsgContent.vue'

const props = defineProps<{
  tabs: SubAgentState[]
  activeTabId: string | null
  /** 父会话定位坐标:子 agent 图片协议 URL 需父 projectId/sessionId + agentId(=activeTabId) */
  projectId?: string | null
  sessionId?: string | null
}>()

// 子 agent 会话级图片定位上下文;agentId 取当前激活 tab,协议 URL 追加 ?agent=
const imageLocator = computed<ImageLocator | null>(() => {
  if (!props.projectId || !props.sessionId || !props.activeTabId) return null
  return { projectId: props.projectId, sessionId: props.sessionId, agentId: props.activeTabId }
})
provide(IMAGE_LOCATOR, imageLocator)

const emit = defineEmits<{
  select: [agentId: string]
  closeTab: [agentId: string]
  closeAll: []
}>()

const { t } = useI18n()

const activeAgent = computed(() =>
  props.tabs.find(a => a.meta.agent_id === props.activeTabId) ?? null,
)

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
  <div class="h-full flex flex-col bg-card">
    <!-- Tab 栏 -->
    <div class="shrink-0 flex items-center border-b border-border overflow-x-auto">
      <div
        v-for="tab in tabs"
        :key="tab.meta.agent_id"
        class="group flex items-center gap-1 px-2.5 py-1.5 text-[11px] border-r border-border cursor-pointer shrink-0 max-w-48"
        :class="tab.meta.agent_id === activeTabId
          ? 'bg-card text-foreground'
          : 'bg-background text-muted-foreground hover:text-foreground hover:bg-card/50'"
        @click="emit('select', tab.meta.agent_id)"
      >
        <span class="i-carbon-bot w-3 h-3 shrink-0" />
        <span class="truncate">{{ tab.meta.description || tab.meta.agent_type || 'Agent' }}</span>
        <button
          class="w-3.5 h-3.5 flex items-center justify-center rounded opacity-0 group-hover:opacity-100 hover:bg-muted shrink-0 transition-opacity"
          @click.stop="emit('closeTab', tab.meta.agent_id)"
        >
          <span class="i-carbon-close w-2.5 h-2.5" />
        </button>
      </div>
      <div class="flex-1" />
      <button
        class="w-6 h-6 flex items-center justify-center hover:bg-muted transition-colors shrink-0 mx-0.5"
        :title="t('common.close')"
        @click="emit('closeAll')"
      >
        <span class="i-carbon-close w-3 h-3" />
      </button>
    </div>

    <!-- 无选中 -->
    <div v-if="!activeAgent" class="flex-1 flex items-center justify-center">
      <p class="text-muted-foreground text-xs">{{ t('subAgent.panel') }}</p>
    </div>

    <!-- 加载态 -->
    <div v-else-if="activeAgent.loading" class="flex-1 flex items-center justify-center">
      <p class="text-muted-foreground text-xs">{{ t('session.loadingChat') }}</p>
    </div>

    <!-- 消息流 -->
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
  </div>
</template>
