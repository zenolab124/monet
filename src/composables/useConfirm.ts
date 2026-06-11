import { ref } from 'vue'

/**
 * 轻量确认弹窗(Promise 风格)。组件 ConfirmDialog.vue 挂在 App 根部消费此状态。
 * Tauri WebView 禁用了原生 window.confirm,自制 Paper 风格 modal。
 */

const visible = ref(false)
const message = ref('')
const confirmLabel = ref('确认')

let resolver: ((v: boolean) => void) | null = null

/** 弹出确认框,resolve true=确认 / false=取消 */
function confirm(msg: string, okLabel = '确认'): Promise<boolean> {
  // 已有弹窗时直接取消旧的(不排队,后到优先)
  resolver?.(false)
  message.value = msg
  confirmLabel.value = okLabel
  visible.value = true
  return new Promise(resolve => {
    resolver = resolve
  })
}

function settle(value: boolean) {
  visible.value = false
  resolver?.(value)
  resolver = null
}

export function useConfirm() {
  return { visible, message, confirmLabel, confirm, settle }
}
