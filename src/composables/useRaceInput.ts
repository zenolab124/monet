import { ref, computed, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { WorkbenchTab } from './useWorkbench'
import { useWorkbench } from './useWorkbench'
import { useStreaming, getStream } from './useStreaming'
import { useImageInput } from './useImageInput'
import { refreshChannels, resolveChannel } from './useChannels'
import { useAppDefaults } from './useAppDefaults'
import { getSessionSettings, ADVISOR_MAIN_MODEL } from './useSessionSettings'
import { parseCommand } from './useSlashCommands'

export function useRaceInput(tab: Ref<WorkbenchTab>) {
  const inputText = ref('')
  const textareaRef = ref<HTMLTextAreaElement>()
  const imageInput = useImageInput({ pasteTarget: textareaRef })
  const slashError = ref<string | null>(null)

  const { sendMessage, stopStreaming } = useStreaming()
  const { addRaceLane } = useWorkbench()
  const { appDefaults } = useAppDefaults()

  const anyStreaming = computed(() => {
    const race = tab.value.race
    if (!race) return false
    return race.lanes.some(lane => getStream(lane.sessionId).streaming)
  })

  const streamingCount = computed(() => {
    const race = tab.value.race
    if (!race) return 0
    return race.lanes.filter(lane => getStream(lane.sessionId).streaming).length
  })

  async function broadcastSend() {
    const race = tab.value.race
    if (!race) return
    const text = inputText.value.trim()
    if (!text && !imageInput.images.value.length) return

    const parsed = parseCommand(text)
    if (parsed.kind === 'invalid') {
      slashError.value = parsed.reason
      return
    }
    if (parsed.kind === 'native') return
    slashError.value = null

    inputText.value = ''
    if (textareaRef.value) textareaRef.value.style.height = 'auto'

    const images = imageInput.images.value.length ? await imageInput.toImageBlocks() : undefined
    imageInput.clearImages()
    await refreshChannels()

    const promises = race.lanes.map(lane => {
      const settings = getSessionSettings(lane.sessionId)
      const advisor = settings.advisor
      return sendMessage(lane.sessionId, race.cwd, text, {
        model: advisor ? ADVISOR_MAIN_MODEL : (settings.modelId ?? undefined),
        effort: settings.effort ?? appDefaults.value.effort,
        channel: resolveChannel(settings.channelId),
        advisor,
        images,
        permissionMode: settings.permissionMode,
      })
    })
    await Promise.allSettled(promises)
  }

  function stopAll() {
    const race = tab.value.race
    if (!race) return
    for (const lane of race.lanes) {
      if (getStream(lane.sessionId).streaming) {
        stopStreaming(lane.sessionId)
      }
    }
  }

  async function forkNewLane() {
    const race = tab.value.race
    if (!race || race.lanes.length === 0) return
    const { draftCwd, state } = useWorkbench()
    const sourceLane = race.lanes[0]
    const isDraft = !!state.value.drafts[sourceLane.sessionId]
    const newSessionId = crypto.randomUUID()
    try {
      if (!isDraft) {
        await invoke('fork_session', {
          sourceSessionId: sourceLane.sessionId,
          newSessionId,
          cwd: race.cwd,
        })
      } else {
        state.value.drafts[newSessionId] = race.cwd
      }
      addRaceLane(tab.value.id, newSessionId)
    } catch (e) {
      console.error('Fork failed:', e)
    }
  }

  return { inputText, textareaRef, imageInput, slashError, anyStreaming, streamingCount, broadcastSend, stopAll, forkNewLane }
}
