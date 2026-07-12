import { ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { readMigratedStorage } from '../utils/storageMigrate'

const STORAGE_KEY = 'monet-zoom'
const LEGACY_STORAGE_KEY = 'cc-space-zoom' // 旧 key,一次性迁移读取用
const DEFAULT_ZOOM = 1
const MIN_ZOOM = 0.7
const MAX_ZOOM = 1.5
const STEP = 0.05

const zoomLevel = ref(loadZoom())

function loadZoom(): number {
  const raw = Number(readMigratedStorage(STORAGE_KEY, LEGACY_STORAGE_KEY))
  return clamp(raw || DEFAULT_ZOOM)
}

function clamp(v: number): number {
  return Math.round(Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, v)) * 100) / 100
}

export async function applyZoom(factor?: number) {
  const f = factor ?? zoomLevel.value
  try {
    await getCurrentWebview().setZoom(f)
  } catch {}
}

async function setZoom(factor: number) {
  const clamped = clamp(factor)
  zoomLevel.value = clamped
  localStorage.setItem(STORAGE_KEY, String(clamped))
  await applyZoom(clamped)
}

export function useZoom() {
  return {
    zoomLevel,
    setZoom,
    MIN_ZOOM,
    MAX_ZOOM,
    STEP,
  }
}
