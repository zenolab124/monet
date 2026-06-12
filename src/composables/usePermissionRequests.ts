/**
 * 权限请求队列管理（v2.1.0 起按会话隔离）
 *
 * 与 Rust 端 MCP server 的事件契约:
 *
 *  Rust → Front: Tauri Event `permission-request`,payload:
 *    { requestId, sessionId, toolName, input, timestamp }
 *
 *  Front → Rust: Tauri Command `respond_permission`,参数:
 *    { requestId: string, allow: boolean, message: string | null }
 *
 * 模块级单例:整个 app 一套队列,initPermissionListener 在应用根组件挂一次。
 * 队列同时是通知层持久型 toast 与左列决策条的事实源——任一处处理,各处同步消失（FR-006 同源同步）。
 */

import { ref, computed, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { checkDangerous, type DangerousFlag } from '@/utils/dangerousOps'

/** 队列中的单条权限请求(已扩展前端字段) */
export interface PermissionRequest {
  /** Rust 端生成的唯一请求 ID,响应时回传 */
  requestId: string
  /** 所属会话（Rust 端按流式会话注入） */
  sessionId: string
  /** 工具名,如 Edit / Bash / Write */
  toolName: string
  /** 工具调用 input 对象 */
  input: Record<string, unknown>
  /** Rust 端发出请求的时间戳(ms) */
  timestamp: number
  /** 危险标识(前端在入队前提前计算) */
  danger: DangerousFlag | null
}

/** Rust → Front 事件 payload 形状 */
interface PermissionRequestEventPayload {
  requestId: string
  sessionId: string
  toolName: string
  input: Record<string, unknown>
  timestamp: number
}

/** 用户决策类型 */
export type PermissionDecision = 'allow_once' | 'allow_session' | 'deny'

/** 权限请求队列(模块级单例,跨会话混合,按 sessionId 过滤消费) */
const queue = ref<PermissionRequest[]>([])

/**
 * "允许此会话"缓存:
 *   key = `${sessionId}::${toolName}::${keyParam}`
 *   keyParam = file_path / command / url 之一(同一 toolName 取首个非空)
 *
 * 命中即 Rust 端无需再问前端,直接 invoke('respond_permission', allow:true) 放行。
 */
const sessionAllowList = new Map<string, boolean>()

/**
 * 计算 sessionAllow 缓存键。
 *   - 有细分参数(file_path/command/url):按参数缓存,同工具不同目标分别记忆
 *     (Bash 总带 command、Edit/Write 带 file_path,危险操作仍按具体目标细分,不放大授权面)
 *   - 无细分参数(Workflow/AskUserQuestion/ExitPlanMode 等交互/编排工具):退化为
 *     「本会话此工具整体放行」。这类工具没有可细分的危险维度,旧实现因取不到参数
 *     返回 null → 不缓存 →「允许此会话」对它们形同虚设,每次必再问。
 */
function buildSessionKey(
  sid: string,
  toolName: string,
  input: Record<string, unknown>,
): string {
  const pick = (k: string): string | null => {
    const v = input[k]
    return typeof v === 'string' && v.length > 0 ? v : null
  }
  const param = pick('file_path') ?? pick('command') ?? pick('url')
  return param ? `${sid}::${toolName}::${param}` : `${sid}::${toolName}`
}

/** 全局监听句柄 */
let unlisten: UnlistenFn | null = null

/**
 * 启动监听。整个 app 生命周期调用一次(建议在 App.vue onMounted)。
 * sessionAllowList 命中判定直接用 payload.sessionId（多会话并行下不依赖"当前选中"）。
 */
export async function initPermissionListener(): Promise<void> {
  if (unlisten) return

  unlisten = await listen<PermissionRequestEventPayload>(
    'permission-request',
    async (e) => {
      const { requestId, sessionId, toolName, input, timestamp } = e.payload

      // 先查 session allow list:命中则不入队、直接放行
      if (sessionId) {
        const key = buildSessionKey(sessionId, toolName, input)
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
        sessionId: sessionId ?? '',
        toolName,
        input,
        timestamp,
        danger: checkDangerous(toolName, input),
      })
    },
  )
}

/** 停止监听(测试或 app 退出时) */
export async function disposePermissionListener(): Promise<void> {
  unlisten?.()
  unlisten = null
}

/** 某会话的待决请求队列（响应式过滤视图） */
export function queueForSession(
  sessionId: Ref<string | null> | ComputedRef<string | null>,
) {
  return computed(() => queue.value.filter(r => r.sessionId === sessionId.value))
}

/** 某会话的当前(队首)请求 */
export function currentForSession(
  sessionId: Ref<string | null> | ComputedRef<string | null>,
) {
  return computed<PermissionRequest | null>(
    () => queue.value.find(r => r.sessionId === sessionId.value) ?? null,
  )
}

/** 判断某会话是否有待决权限（监控卡状态派生用,非响应式入参版） */
export function hasPendingPermission(sessionId: string): boolean {
  return queue.value.some(r => r.sessionId === sessionId)
}

/**
 * 响应指定请求（toast / 左列决策条 / 列内权限卡三入口共用,同源同步）。
 *
 * @param requestId 要响应的请求
 * @param decision  用户决策
 */
export async function respondRequest(
  requestId: string,
  decision: PermissionDecision,
): Promise<void> {
  const req = queue.value.find(r => r.requestId === requestId)
  if (!req) return

  // 先出队再 invoke,避免 invoke 失败时卡住队列
  queue.value = queue.value.filter(r => r.requestId !== requestId)

  // allow_session:写入缓存
  if (decision === 'allow_session' && req.sessionId) {
    const key = buildSessionKey(req.sessionId, req.toolName, req.input)
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
 * 拒绝某会话的所有 pending 请求。
 *
 * 用于该会话流式中断(Esc / stopStreaming)时清场。
 */
export async function denyAllForSession(sessionId: string): Promise<void> {
  const pending = queue.value.filter(r => r.sessionId === sessionId)
  queue.value = queue.value.filter(r => r.sessionId !== sessionId)
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
    /** 全量队列(只读,跨会话) */
    queue,
    /** 按会话过滤 */
    queueForSession,
    currentForSession,
    /** 响应指定请求 */
    respondRequest,
    /** 某会话流式中断时拒绝其全部 pending */
    denyAllForSession,
    /** 清空 sessionAllow 缓存 */
    clearSessionAllowList,
  }
}
