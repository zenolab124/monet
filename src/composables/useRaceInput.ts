import { ref, computed, type Ref } from 'vue'
import type { WorkbenchTab } from './useWorkbench'
import { useWorkbench } from './useWorkbench'
import { useStreaming, getStream } from './useStreaming'
import { useImageInput } from './useImageInput'
import { refreshChannels } from './useChannels'
import { resolveRunConfig } from './useRunConfig'
import { getSessionSettings } from './useSessionSettings'
import { parseCommand } from './useSlashCommands'

export function useRaceInput(tab: Ref<WorkbenchTab>) {
  const inputText = ref('')
  const textareaRef = ref<HTMLTextAreaElement>()
  // 拖拽收图区由组件侧绑定(整个赛马区,拖到任意位置都进共享输入)
  const dropAreaRef = ref<HTMLElement>()
  const imageInput = useImageInput({ pasteTarget: textareaRef, dropTarget: dropAreaRef })
  const slashError = ref<string | null>(null)

  const { sendMessage, stopStreaming } = useStreaming()
  const { addRaceLane, forkSourceOf } = useWorkbench()

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
    if (parsed.kind === 'native' || parsed.kind === 'terminal') return
    slashError.value = null

    inputText.value = ''
    if (textareaRef.value) textareaRef.value.style.height = 'auto'

    const images = imageInput.images.value.length ? await imageInput.toImageBlocks() : undefined
    imageInput.clearImages()
    await refreshChannels()

    const promises = race.lanes.map(lane => {
      const settings = getSessionSettings(lane.sessionId)
      const rc = resolveRunConfig(settings)
      return sendMessage(lane.sessionId, race.cwd, text, {
        model: rc.model,
        effort: rc.effort ?? null,
        channel: rc.channelId,
        advisor: settings.advisor,
        chrome: settings.chrome,
        forkSource: forkSourceOf(lane.sessionId) ?? undefined,
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

  function forkNewLane() {
    const race = tab.value.race
    if (!race || race.lanes.length === 0) return
    const { registerFork, state } = useWorkbench()
    const sourceLane = race.lanes[0]
    const isDraft = !!state.value.drafts[sourceLane.sessionId]
    const newSessionId = crypto.randomUUID()
    // 懒分叉:非草稿源登记意图(落盘由 CLI --fork-session 完成);草稿源无历史,仅登记草稿
    if (!isDraft) {
      registerFork(newSessionId, sourceLane.sessionId, race.cwd)
    } else {
      state.value.drafts[newSessionId] = race.cwd
    }
    addRaceLane(tab.value.id, newSessionId)
  }

  return { inputText, textareaRef, dropAreaRef, imageInput, slashError, anyStreaming, streamingCount, broadcastSend, stopAll, forkNewLane }
}
