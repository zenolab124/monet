import { ref } from 'vue'
import i18n from '../locales'

/**
 * 轻量确认弹窗(Promise 风格)。组件 ConfirmDialog.vue 挂在 App 根部消费此状态。
 * Tauri WebView 禁用了原生 window.confirm,自制 Paper 风格 modal。
 */

export type ConfirmAction = { label: string; value: string; style?: 'primary' | 'destructive' | 'success' }

const visible = ref(false)
const message = ref('')
const confirmLabel = ref(i18n.global.t('common.confirm'))
const actions = ref<ConfirmAction[]>([])

let resolver: ((v: boolean) => void) | null = null
let multiResolver: ((v: string | null) => void) | null = null

/** 弹出确认框,resolve true=确认 / false=取消 */
function confirm(msg: string, okLabel = i18n.global.t('common.confirm')): Promise<boolean> {
  resolver?.(false)
  multiResolver?.(null)
  message.value = msg
  confirmLabel.value = okLabel
  actions.value = []
  visible.value = true
  return new Promise(resolve => {
    resolver = resolve
    multiResolver = null
  })
}

/** 多选项确认框,resolve 对应 action.value 或 null(取消) */
function confirmMulti(msg: string, opts: ConfirmAction[]): Promise<string | null> {
  resolver?.(false)
  multiResolver?.(null)
  message.value = msg
  actions.value = opts
  visible.value = true
  return new Promise(resolve => {
    multiResolver = resolve
    resolver = null
  })
}

function settle(value: boolean) {
  visible.value = false
  resolver?.(value)
  resolver = null
}

function settleMulti(value: string | null) {
  visible.value = false
  multiResolver?.(value)
  multiResolver = null
}

export function useConfirm() {
  return { visible, message, confirmLabel, actions, confirm, confirmMulti, settle, settleMulti }
}
