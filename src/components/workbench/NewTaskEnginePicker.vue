<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useChannels, channelSupportsEngine, refreshChannels } from '@/composables/useChannels'
import { useNotifications } from '@/composables/useNotifications'
import { useEngineNotices } from '@/composables/useEngineNotices'
import { useProjects } from '@/composables/useProjects'
import { useWorkbench } from '@/composables/useWorkbench'
import { createSession } from '@/engines/client'
import { sameInstance } from '@/engines/identity'
import { buildEngineChoices } from '@/engines/engineChoices'
import { sessionUiId, usesNativeSessionSurface } from '@/engines/integration'
import { useEngines } from '@/engines/useEngines'
import type { EngineDescriptor, EngineInstanceId, ProjectRef } from '@/engines/types'
import type { Project } from '@/types'
import EngineChoicePanel from './EngineChoicePanel.vue'
import NewTaskDirectoryPicker from './NewTaskDirectoryPicker.vue'

/**
 * 新任务的引擎选择。会话位置 = 实例(引擎@主机) + 该主机上的目录,不跨主机关联项目:
 * - 已选项目(cwd 非空):只列出项目所在主机上的实例;
 * - 空白新建(cwd 为 null):先选实例,再在该主机上选目录。
 */
const props = defineProps<{
  sessionId: string
  cwd: string | null
  /** 项目所在的远端实例;本机为 null */
  host: EngineInstanceId | null
}>()

const { t } = useI18n()
const { projects } = useProjects()
const { engines, health, loading, errors, refreshEngines } = useEngines()
const { channels, defaultSessionChannels } = useChannels()
const {
  promotePendingTaskToDraft,
  stageDraftSession,
  stageEngineDraft,
  replaceWorkbenchSession,
  discardStagedSession,
} = useWorkbench()
const { notifyTransient } = useNotifications()
const { codexCreateDelayRisk } = useEngineNotices()
const selectingEngine = ref<string | null>(null)
const selectingHint = ref<string | null>(null)
const error = ref<string | null>(null)
let slowHintTimer: ReturnType<typeof setTimeout> | null = null

const isRemote = (descriptor: EngineDescriptor | null) => descriptor?.capabilities.facets.remoteWorkspace === true
const blankChoiceId = ref<string | null>(null)

// 每个引擎实例一个选项;选项 id 是实例 key,原生实例的草稿据此记录归属
const allChoices = computed(() => buildEngineChoices({
  engines: engines.value,
  health: health.value,
  errors: errors.value,
  loading: loading.value,
}).map(choice => ({
  ...choice,
  description: t(`workbench.enginePicker.${choice.engineId === 'claude-code' ? 'claudeDescription' : 'codexDescription'}`),
})))
// 已选项目时只保留该主机上的实例(未注册的占位项属于本机)
const choices = computed(() => props.cwd === null
  ? allChoices.value
  : allChoices.value.filter(choice => props.host
    ? !!choice.descriptor && sameInstance(choice.descriptor.instance, props.host)
    : !isRemote(choice.descriptor)))
const blankChoice = computed(() => allChoices.value.find(choice => choice.id === blankChoiceId.value) ?? null)
const hostLabel = computed(() => props.host
  ? allChoices.value.find(choice => choice.descriptor && sameInstance(choice.descriptor.instance, props.host!))?.label ?? null
  : null)
const panelTitle = computed(() => t(props.cwd === null ? 'workbench.newTask.blankTitle' : 'workbench.enginePicker.title'))
const panelDescription = computed(() => props.cwd === null
  ? t('workbench.newTask.blankDescription')
  : hostLabel.value
    ? t('workbench.newTask.hostOnly', { host: hostLabel.value })
    : t('workbench.enginePicker.description'))

function projectCwd(project: Project): string | null {
  return project.sessions.find(session => session.cwd)?.cwd ?? project.source_path ?? null
}

function projectReference(engine: EngineDescriptor, cwd: string): ProjectRef {
  return projects.value.find(project =>
    !!project.reference
    && sameInstance(project.reference.engine, engine.instance)
    && projectCwd(project) === cwd,
  )?.reference ?? { engine: engine.instance, nativeId: cwd }
}

function defaultChannelForEngine(engine: EngineDescriptor): string | null {
  const engineId = engine.instance.engineId
  if (engineId !== 'claude-code' && engineId !== 'codex') return null
  const channelId = defaultSessionChannels.value[engineId]
  if (!channelId) return null
  const channel = channels.value.find(item => item.id === channelId)
  return channel?.enabled
    && channel.scope !== 'agent-only'
    && channelSupportsEngine(channel, engineId)
    ? channelId
    : null
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause)
}

function selectEngine(choiceId: string) {
  const choice = choices.value.find(item => item.id === choiceId)
  if (!choice?.available || !choice.descriptor || selectingEngine.value) return
  error.value = null
  if (props.cwd === null) blankChoiceId.value = choice.id
  else void startTask(choice.id, props.cwd)
}

async function startTask(choiceId: string, cwd: string) {
  const choice = allChoices.value.find(item => item.id === choiceId)
  if (!choice?.available || !choice.descriptor || selectingEngine.value) return
  selectingEngine.value = choice.id
  error.value = null
  try {
    const instance = choice.descriptor.instance
    const nativeId = crypto.randomUUID()
    const nativeSessionId = sessionUiId({ engine: instance, nativeId })
    if (usesNativeSessionSurface(instance) && nativeSessionId !== nativeId) {
      // 结构化身份的原生实例（远端主机）：目录来自该主机的项目或已校验的远端路径；
      // 草稿 id 用结构化 id 包住新 uuid，会话落盘后界面 id 不变
      const replacementSessionId = stageDraftSession(cwd, nativeSessionId, instance)
      if (!replaceWorkbenchSession(props.sessionId, replacementSessionId)) {
        discardStagedSession(replacementSessionId)
        throw new Error(t('workbench.enginePicker.expired'))
      }
    } else if (usesNativeSessionSurface(instance)) {
      if (!promotePendingTaskToDraft(props.sessionId, instance, cwd)) throw new Error(t('workbench.enginePicker.expired'))
    } else {
      await refreshChannels()
      const attachedChannel = defaultChannelForEngine(choice.descriptor)
      const project = projectReference(choice.descriptor, cwd)
      if (choice.engineId === 'codex') {
        slowHintTimer = setTimeout(() => {
          selectingHint.value = t(codexCreateDelayRisk.value
            ? 'workbench.enginePicker.codexVersionRefresh'
            : 'workbench.enginePicker.codexSlowStart')
        }, 800)
      }
      const created = await createSession(project, cwd, attachedChannel ? { channelId: attachedChannel } : {})
      const replacementSessionId = sessionUiId(created.session)
      stageEngineDraft(replacementSessionId, {
        reference: created.session,
        project,
        engineName: choice.descriptor.displayName,
        cwd,
        sourceMeta: created.sourceMeta,
        attachedChannel,
        attachedCapabilityFingerprint: created.capabilityFingerprint,
      })
      if (!replaceWorkbenchSession(props.sessionId, replacementSessionId)) {
        discardStagedSession(replacementSessionId)
        throw new Error(t('workbench.enginePicker.expired'))
      }
    }
    notifyTransient(t('workbench.rail.newSessionReady'), t('workbench.rail.newSessionHint'))
  } catch (cause) {
    const message = errorMessage(cause)
    error.value = message
    notifyTransient(t('common.newSessionFailed'), message)
  } finally {
    if (slowHintTimer) clearTimeout(slowHintTimer)
    slowHintTimer = null
    selectingHint.value = null
    selectingEngine.value = null
  }
}

onMounted(() => {
  void refreshChannels()
  if (engines.value.length === 0) void refreshEngines()
})

onUnmounted(() => {
  if (slowHintTimer) clearTimeout(slowHintTimer)
})
</script>

<template>
  <NewTaskDirectoryPicker
    v-if="blankChoice?.descriptor"
    :descriptor="blankChoice.descriptor"
    :busy="!!selectingEngine"
    @pick="startTask(blankChoice.id, $event)"
    @back="blankChoiceId = null"
  />
  <EngineChoicePanel
    v-else
    :title="panelTitle"
    :description="panelDescription"
    :choices="choices"
    :selecting-engine="selectingEngine"
    :selecting-hint="selectingHint"
    :error="error"
    @select="selectEngine"
  />
</template>
