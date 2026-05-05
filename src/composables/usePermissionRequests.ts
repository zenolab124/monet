/**
 * 权限请求队列管理(FR-003)
 *
 * 与 Rust 端 MCP server 的事件契约:
 *
 *  Rust → Front: Tauri Event `permission-request`,payload:
 *    { requestId: string, toolName: string, input: Record<string, unknown>, timestamp: number }
 *
 *  Front → Rust: Tauri Command `respond_permission`,参数:
 *    { requestId: string, allow: boolean, message: string | null }
 *
 * 模块级单例:整个 app 一套队列,initPermissionListener 在应用根组件挂一次。
 */

import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { checkDangerous, type DangerousFlag } from '@/utils/dangerousOps'

/** 队列中的单条权限请求(已扩展前端字段) */
export interface PermissionRequest {
  /** Rust 端生成的唯一请求 ID,响应时回传 */
  requestId: string
  /** 工具名,如 Edit / Bash / Write */
  toolName: string
  /** 工具调用 input 对象 */
  input: Record<string, unknown>
  /** Rust 端发出请求的时间戳(ms) */
  timestamp: number
  /** 危险标识(前端在入队前提前计算) */
  danger: DangerousFlag | null
  /** 超时拒绝时刻(ms);timestamp + 60_000 */
  timeoutAt: number
}

/** Rust → Front 事件 payload 形状 */
interface PermissionRequestEventPayload {
  requestId: string
  toolName: string
  input: Record<string, unknown>
  timestamp: number
}

/** 用户决策类型 */
export type PermissionDecision = 'allow_once' | 'allow_session' | 'deny'

/** 权限请求队列(模块级单例) */
const queue = ref<PermissionRequest[]>([])

/** 当前显示的请求(队首),无则 null */
const current = computed<PermissionRequest | null>(() => queue.value[0] ?? null)

/**
 * "允许此会话"缓存:
 *   key = `${sessionId}::${toolName}::${keyParam}`
 *   keyParam = file_path / command / url 之一(同一 toolName 取首个非空)
 *
 * 命中即 Rust 端无需再问前端,直接 invoke('respond_permission', allow:true) 放行。
 */
const sessionAllowList = new Map<string, boolean>()

/**
 * 计算 sessionAllow 缓存键。无可用关键参数时返回 null,
 * 此时不进缓存(防止"模糊放行"风险)。
 */
function buildSessionKey(
  sid: string,
  toolName: string,
  input: Record<string, unknown>,
): string | null {
  const pick = (k: string): string | null => {
    const v = input[k]
    return typeof v === 'string' && v.length > 0 ? v : null
  }
  const param = pick('file_path') ?? pick('command') ?? pick('url')
  if (!param) return null
  return `${sid}::${toolName}::${param}`
}

/** 全局监听句柄 */
let unlisten: UnlistenFn | null = null
let unlistenTimeout: UnlistenFn | null = null

/**
 * 启动监听。整个 app 生命周期调用一次(建议在 App.vue onMounted)。
 *
 * @param getActiveSessionId 取当前激活会话 ID 的函数(用于 sessionAllowList 命中)
 */
export async function initPermissionListener(
  getActiveSessionId: () => string | null,
): Promise<void> {
  if (unlisten) return

  // Rust 60s 超时事件:从队列移除该 requestId(Rust 已自动 deny,前端不需再 invoke)
  unlistenTimeout = await listen<{ requestId: string }>(
    'permission-timeout',
    (e) => {
      queue.value = queue.value.filter(r => r.requestId !== e.payload.requestId)
    },
  )

  unlisten = await listen<PermissionRequestEventPayload>(
    'permission-request',
    async (e) => {
      const { requestId, toolName, input, timestamp } = e.payload

      // 先查 session allow list:命中则不入队、直接放行
      const sid = getActiveSessionId()
      if (sid) {
        const key = buildSessionKey(sid, toolName, input)
        if (key && sessionAllowList.get(key)) {
          try {
            await invoke('respond_permission', {
              requestId,
              allow: true,
              message: null,
            })
          } catch (_) {
            // 响应失败:Rust 端可能已经超时,不再处理
          }
          return
        }
      }

      // 入队
      queue.value.push({
        requestId,
        toolName,
        input,
        timestamp,
        danger: checkDangerous(toolName, input),
        timeoutAt: timestamp + 60_000,
      })
    },
  )
}

/** 停止监听(测试或 app 退出时) */
export async function disposePermissionListener(): Promise<void> {
  unlisten?.()
  unlistenTimeout?.()
  unlisten = null
  unlistenTimeout = null
}

/**
 * 响应当前(队首)请求。
 *
 * @param decision 用户决策
 * @param sid      当前激活会话 ID(allow_session 必需)
 */
export async function respondCurrent(
  decision: PermissionDecision,
  sid: string | null,
): Promise<void> {
  const req = queue.value[0]
  if (!req) return

  // 先出队再 invoke,避免 invoke 失败时卡住队列
  queue.value = queue.value.slice(1)

  // allow_session:写入缓存
  if (decision === 'allow_session' && sid) {
    const key = buildSessionKey(sid, req.toolName, req.input)
    if (key) sessionAllowList.set(key, true)
  }

  try {
    await invoke('respond_permission', {
      requestId: req.requestId,
      allow: decision !== 'deny',
      message: decision === 'deny' ? '用户拒绝' : null,
    })
  } catch (_) {
    // 响应失败:不再补救,Rust 端会按超时处理
  }
}

/**
 * 拒绝所有 pending 请求。
 *
 * 用于流式中断(Esc / stopStreaming)时清场。
 */
export async function denyAllPending(): Promise<void> {
  const pending = queue.value
  queue.value = []
  for (const req of pending) {
    try {
      await invoke('respond_permission', {
        requestId: req.requestId,
        allow: false,
        message: '流式已中断',
      })
    } catch (_) {
      // ignore
    }
  }
}

/**
 * 清空 sessionAllowList。建议时机:
 *   - 会话切换
 *   - 流式结束
 *   - 用户显式重置
 *
 * 由调用方决定何时调,本模块不主动清。
 */
export function clearSessionAllowList(): void {
  sessionAllowList.clear()
}

/** 组件中使用的 hook,返回响应式状态与操作 */
export function usePermissionRequests() {
  return {
    /** 队列(只读) */
    queue,
    /** 当前请求(队首) */
    current,
    /** 响应当前请求 */
    respondCurrent,
    /** 流式中断时拒绝所有 pending */
    denyAllPending,
    /** 清空 sessionAllow 缓存 */
    clearSessionAllowList,
  }
}
