import { listen } from '@tauri-apps/api/event'
import { useConfirm } from './useConfirm'
import { useWorkbench } from './useWorkbench'
import { useUiState } from './useUiState'
import i18n from '../locales'

const t = () => i18n.global.t

export async function initShortcuts() {
  const { confirm } = useConfirm()
  const { state, closeTab, activeTab } = useWorkbench()
  const { activeSection } = useUiState()

  await listen('menu:close-tab', async () => {
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
    }
  })
}
