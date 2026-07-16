import { ref } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

export type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'restarting' | 'up-to-date' | 'error'

const status = ref<UpdateStatus>('idle')
const newVersion = ref('')
const releaseNotes = ref('')
const errorMessage = ref('')
const downloadProgress = ref(0)

let pendingUpdate: Update | null = null

async function checkForUpdate() {
  status.value = 'checking'
  errorMessage.value = ''
  try {
    const update = await check()
    if (update) {
      pendingUpdate = update
      newVersion.value = update.version
      releaseNotes.value = update.body ?? ''
      status.value = 'available'
    } else {
      status.value = 'up-to-date'
    }
  } catch (e) {
    errorMessage.value = String(e)
    status.value = 'error'
  }
}

async function downloadAndInstall() {
  if (!pendingUpdate) return
  status.value = 'downloading'
  downloadProgress.value = 0
  try {
    let totalLen = 0
    let downloaded = 0
    await pendingUpdate.downloadAndInstall((event) => {
      if (event.event === 'Started' && event.data.contentLength) {
        totalLen = event.data.contentLength
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength
        downloadProgress.value = totalLen ? Math.round((downloaded / totalLen) * 100) : 0
      } else if (event.event === 'Finished') {
        downloadProgress.value = 100
      }
    })
    // downloadAndInstall resolve = 新版已替换就位,但 Tauri 不会自动重启——
    // 必须显式 relaunch 才生效(缺此调用曾致 UI 永卡"下载 100%")
    status.value = 'restarting'
    await relaunch()
  } catch (e) {
    errorMessage.value = String(e)
    status.value = 'error'
  }
}

async function initAutoCheck() {
  await new Promise(r => setTimeout(r, 5000))
  try {
    const update = await check()
    if (update) {
      pendingUpdate = update
      newVersion.value = update.version
      releaseNotes.value = update.body ?? ''
      status.value = 'available'
    }
  } catch {
    // 静默失败，不打扰用户
  }
}

export function useUpdater() {
  return {
    status,
    newVersion,
    releaseNotes,
    errorMessage,
    downloadProgress,
    checkForUpdate,
    downloadAndInstall,
    initAutoCheck,
  }
}
