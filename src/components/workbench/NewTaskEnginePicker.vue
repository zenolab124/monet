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
import type { EngineDescriptor, ProjectRef } from '@/engines/types'
import type { Project } from '@/types'
import EngineChoicePanel from './EngineChoicePanel.vue'

const props = defineProps<{
  sessionId: string
  cwd: string
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

// 每个引擎实例一个选项;选项 id 是实例 key,原生实例的草稿据此记录归属
const choices = computed(() => buildEngineChoices({
  engines: engines.value,
  health: health.value,
  errors: errors.value,
  loading: loading.value,
}).map(choice => ({
  ...choice,
  description: t(`workbench.enginePicker.${choice.engineId === 'claude-code' ? 'claudeDescription' : 'codexDescription'}`),
})))

function projectCwd(project: Project): string | null {
  return project.sessions.find(session => session.cwd)?.cwd ?? project.source_path ?? null
}

function projectReference(engine: EngineDescriptor): ProjectRef {
  return projects.value.find(project =>
    !!project.reference
    && sameInstance(project.reference.engine, engine.instance)
    && projectCwd(project) === props.cwd,
  )?.reference ?? { engine: engine.instance, nativeId: props.cwd }
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

async function selectEngine(choiceId: string) {
  const choice = choices.value.find(item => item.id === choiceId)
  if (!choice) return
  if (!choice.available || !choice.descriptor || selectingEngine.value) return
  selectingEngine.value = choice.id
  error.value = null
  try {
    const instance = choice.descriptor.instance
    const nativeId = crypto.randomUUID()
    const nativeSessionId = sessionUiId({ engine: instance, nativeId })
    if (usesNativeSessionSurface(instance) && nativeSessionId !== nativeId) {
      // 结构化身份的原生实例（远端主机）：目录必须是该实例已有的项目（不浏览远端文件夹），
      // 草稿 id 用结构化 id 包住新 uuid，会话落盘后界面 id 不变
      const known = projects.value.some(project => project.engine
        && sameInstance(project.engine, instance)
        && projectCwd(project) === props.cwd)
      if (!known) throw new Error(t('remote.cwdNotOnHost', { host: choice.descriptor.displayName }))
      const replacementSessionId = stageDraftSession(props.cwd, nativeSessionId, instance)
      if (!replaceWorkbenchSession(props.sessionId, replacementSessionId)) {
        discardStagedSession(replacementSessionId)
        throw new Error(t('workbench.enginePicker.expired'))
      }
    } else if (usesNativeSessionSurface(instance)) {
      if (!promotePendingTaskToDraft(props.sessionId, instance)) throw new Error(t('workbench.enginePicker.expired'))
    } else {
      await refreshChannels()
      const attachedChannel = defaultChannelForEngine(choice.descriptor)
      const project = projectReference(choice.descriptor)
      if (choice.engineId === 'codex') {
        slowHintTimer = setTimeout(() => {
          selectingHint.value = t(codexCreateDelayRisk.value
            ? 'workbench.enginePicker.codexVersionRefresh'
            : 'workbench.enginePicker.codexSlowStart')
        }, 800)
      }
      const created = await createSession(project, props.cwd, attachedChannel ? { channelId: attachedChannel } : {})
      const replacementSessionId = sessionUiId(created.session)
      stageEngineDraft(replacementSessionId, {
        reference: created.session,
        project,
        engineName: choice.descriptor.displayName,
        cwd: props.cwd,
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
  <EngineChoicePanel
    :title="t('workbench.enginePicker.title')"
    :description="t('workbench.enginePicker.description')"
    :choices="choices"
    :selecting-engine="selectingEngine"
    :selecting-hint="selectingHint"
    :error="error"
    @select="selectEngine"
  />
</template>
