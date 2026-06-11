import { computed, type Ref, type ComputedRef } from 'vue'
import { useSessionStream } from './useStreaming'
import { queueForSession } from './usePermissionRequests'
import { useNotifications } from './useNotifications'

/**
 * 会话运行状态派生(FR-003 状态枚举):监控卡状态行与右区列头共用。
 * 状态点颜色:运行三态(流式/Workflow/等待工具)= primary;等待权限 = accent;
 * 出错 = destructive;空闲 = border 色。
 */

export type SessionStatusKey =
  | 'streaming'
  | 'workflow'
  | 'waiting_tool'
  | 'waiting_permission'
  | 'error'
  | 'idle'

export interface SessionStatus {
  key: SessionStatusKey
  label: string
  /** 状态点配色(UnoCSS class) */
  dotClass: string
  /** 运行态脉冲动画 */
  pulse: boolean
  /** 卡片左边框语义('accent' | 'destructive' | null) */
  edge: 'accent' | 'destructive' | null
}

const STATUS_META: Record<SessionStatusKey, Omit<SessionStatus, 'key'>> = {
  streaming: { label: '流式输出', dotClass: 'bg-primary', pulse: true, edge: null },
  workflow: { label: 'Workflow', dotClass: 'bg-primary', pulse: true, edge: null },
  waiting_tool: { label: '等待工具', dotClass: 'bg-primary', pulse: true, edge: null },
  waiting_permission: { label: '等待权限', dotClass: 'bg-accent', pulse: false, edge: 'accent' },
  error: { label: '出错停住', dotClass: 'bg-destructive', pulse: false, edge: 'destructive' },
  idle: { label: '空闲', dotClass: 'bg-border', pulse: false, edge: null },
}

export function useSessionStatus(
  sessionId: Ref<string | null> | ComputedRef<string | null>,
) {
  const stream = useSessionStream(sessionId)
  const perms = queueForSession(sessionId)
  const { persistentToasts } = useNotifications()

  return computed<SessionStatus>(() => {
    const sid = sessionId.value
    let key: SessionStatusKey = 'idle'
    if (perms.value.length > 0) {
      key = 'waiting_permission'
    } else if (
      stream.value.streamError ||
      (sid && persistentToasts.value.some(t => t.kind === 'error' && t.sessionId === sid))
    ) {
      key = 'error'
    } else if (stream.value.streaming) {
      const tool = stream.value.activeTool
      if (tool?.startsWith('Workflow')) key = 'workflow'
      else if (tool) key = 'waiting_tool'
      else key = 'streaming'
    }
    return { key, ...STATUS_META[key] }
  })
}
