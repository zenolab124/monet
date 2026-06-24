import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export interface RoutineDefinition {
  id: string
  name: string
  cronExpression: string
  originalText: string
  prompt: string
  enabled: boolean
  createdAt: string
  lastRun: string | null
  nextRun: string | null
}

export interface RoutineExecutionLog {
  routineId: string
  startedAt: string
  finishedAt: string | null
  exitCode: number | null
  stdout: string
  stderr: string
}

export interface RoutineRow extends RoutineDefinition {
  lastExecution: RoutineExecutionLog | null
  isRunning: boolean
}

const routines = ref<RoutineRow[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
let loaded = false

async function loadRoutines() {
  loading.value = true
  error.value = null
  try {
    routines.value = await invoke<RoutineRow[]>('get_routines')
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function ensureLoaded(force = false) {
  if (loaded && !force) return
  loaded = true
  await loadRoutines()
}

function refresh() {
  ensureLoaded(true)
}

let listenerReady = false
async function initRoutineListener() {
  if (listenerReady) return
  listenerReady = true
  await listen('routine-executed', () => {
    loadRoutines()
  })
  await listen('routines-changed', () => {
    loadRoutines()
  })
}

async function createRoutine(params: {
  name: string
  cronExpression: string
  originalText: string
  prompt: string
  enabled: boolean
}): Promise<RoutineDefinition> {
  const result = await invoke<RoutineDefinition>('create_routine', params)
  await loadRoutines()
  return result
}

async function updateRoutine(
  id: string,
  patch: Partial<Pick<RoutineDefinition, 'name' | 'cronExpression' | 'originalText' | 'prompt' | 'enabled'>>,
): Promise<RoutineDefinition> {
  const result = await invoke<RoutineDefinition>('update_routine', { id, ...patch })
  await loadRoutines()
  return result
}

async function deleteRoutine(id: string): Promise<void> {
  await invoke('delete_routine', { id })
  await loadRoutines()
}

async function runNow(id: string): Promise<void> {
  await invoke('run_routine_now', { id })
}

async function getRoutineLogs(id: string, limit?: number): Promise<RoutineExecutionLog[]> {
  return invoke<RoutineExecutionLog[]>('get_routine_logs', { id, limit: limit ?? 20 })
}

async function parseNaturalSchedule(text: string): Promise<string> {
  return invoke<string>('parse_natural_schedule', { text })
}

export function useRoutines() {
  return {
    routines,
    loading,
    error,
    ensureLoaded,
    refresh,
    initRoutineListener,
    createRoutine,
    updateRoutine,
    deleteRoutine,
    runNow,
    getRoutineLogs,
    parseNaturalSchedule,
  }
}
