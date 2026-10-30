import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (command: string) => (command === 'engine_list' ? { engines: [] } : [])),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ isFocused: async () => true, isVisible: async () => true }),
}))
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => 'denied'),
  sendNotification: vi.fn(),
  registerActionTypes: vi.fn(async () => {}),
  onAction: vi.fn(async () => {}),
}))

function memoryStorage(): Storage {
  const data = new Map<string, string>()
  return {
    get length() { return data.size },
    clear: () => data.clear(),
    getItem: key => data.get(key) ?? null,
    key: index => [...data.keys()][index] ?? null,
    removeItem: key => { data.delete(key) },
    setItem: (key, value) => { data.set(key, String(value)) },
  }
}

describe('leaving the workbench dismisses a session error', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.stubGlobal('localStorage', memoryStorage())
    vi.stubGlobal('document', { addEventListener: vi.fn(), visibilityState: 'visible', hidden: false })
    vi.stubGlobal('window', { innerWidth: 1600, addEventListener: vi.fn() })
  })

  it('clears errors of sessions removed from the workbench and keeps the others', async () => {
    const notifications = await import('../../src/composables/useNotifications')
    const { useWorkbench } = await import('../../src/composables/useWorkbench')
    await notifications.initNotificationLayer()
    const { upsertErrorEvent, persistentToasts } = notifications.useNotifications()
    const workbench = useWorkbench()

    workbench.openSession('session-in-workbench')
    await nextTick()
    upsertErrorEvent('session-in-workbench', 'stream failed', 'stream')
    upsertErrorEvent('session-external', 'api error', 'external')

    workbench.removeSession('session-in-workbench')
    await nextTick()

    const errorSessions = persistentToasts.value.filter(t => t.kind === 'error').map(t => t.sessionId)
    expect(errorSessions).toEqual(['session-external'])
  })
})
