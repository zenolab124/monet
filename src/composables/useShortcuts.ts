import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useConfirm } from './useConfirm'
import { useWorkbench } from './useWorkbench'
import { useUiState } from './useUiState'
import { isMac } from './usePlatform'
import i18n from '../locales'

const t = () => i18n.global.t

export async function initShortcuts() {
  const { confirm } = useConfirm()
  const { state, closeTab, activeTab } = useWorkbench()
  const { activeSection } = useUiState()

  const closeTabAction = async () => {
    if (activeSection.value === 'workbench' && state.value.tabs.length > 1) {
      const tab = activeTab.value
      if (tab.sessionIds.length > 0) {
        const ok = await confirm(
          t()('workbench.closeConfirm', { count: tab.sessionIds.length }),
          t()('common.close'),
        )
        if (!ok) return
      }
      closeTab(tab.id)
    } else if (isMac) {
      // 最后一个 tab / 非工作台域：收起窗口（macOS 标准行为）；
      // 其他平台无 Dock 恢复入口，hide 后窗口无法唤回，不做
      await invoke('hide_main_window')
    }
  }

  const requestQuitAction = async () => {
    // 有活跃会话时确认，避免流式会话被无提示终止；无会话直接退
    const activeSessions = state.value.tabs.reduce((n, tab) => n + tab.sessionIds.length, 0)
    if (activeSessions > 0) {
      const ok = await confirm(
        t()('app.quitConfirm'),
        t()('app.quitFull'),
      )
      if (!ok) return
    }
    await invoke('quit_app')
  }

  await listen('menu:close-tab', closeTabAction)
  await listen('menu:request-quit', requestQuitAction)

  // 非 macOS 无原生菜单（Windows win32 菜单条不跟主题、文案不走 i18n），
  // 菜单 accelerator 由前端 keydown 等价承载
  if (!isMac) {
    window.addEventListener('keydown', (e) => {
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'w') {
        e.preventDefault()
        void closeTabAction()
      }
    })
  }
}
