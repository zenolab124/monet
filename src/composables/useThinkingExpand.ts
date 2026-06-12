import { ref, watch } from 'vue'

/**
 * 思考块全局展开状态:点开任意思考块 = 全部展开,再点 = 全部折叠。
 * 状态持久化到 localStorage,新渲染的思考块(含流式新产出、跨会话)都跟随该状态。
 */

const STORAGE_KEY = 'cc-space:thinking-expanded'

function load(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1'
  } catch (_) {
    return false
  }
}

/** 模块级单例:所有 BlockThinking 实例消费同一份状态 */
const thinkingExpanded = ref(load())

watch(thinkingExpanded, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, v ? '1' : '0')
  } catch (_) {
    // 存储失败静默忽略,仅丢失跨启动记忆
  }
})

export function useThinkingExpand() {
  function toggle() {
    thinkingExpanded.value = !thinkingExpanded.value
  }
  return { thinkingExpanded, toggle }
}
