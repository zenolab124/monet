import { computed, type Ref, type ComputedRef } from 'vue'
import i18n from '../locales'
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

const STATUS_META_STATIC: Record<SessionStatusKey, { dotClass: string; pulse: boolean; edge: 'accent' | 'destructive' | null; labelKey: string }> = {
  streaming: { dotClass: 'bg-primary', pulse: true, edge: null, labelKey: 'session.streaming' },
  workflow: { dotClass: 'bg-primary', pulse: true, edge: null, labelKey: '' },
  waiting_tool: { dotClass: 'bg-primary', pulse: true, edge: null, labelKey: 'session.waitingTool' },
  waiting_permission: { dotClass: 'bg-accent', pulse: false, edge: 'accent', labelKey: 'session.waitingPermission' },
  error: { dotClass: 'bg-destructive', pulse: false, edge: 'destructive', labelKey: 'session.error' },
  idle: { dotClass: 'bg-border', pulse: false, edge: null, labelKey: 'session.idle' },
}

function getStatusMeta(key: SessionStatusKey): Omit<SessionStatus, 'key'> {
  const meta = STATUS_META_STATIC[key]
  const label = meta.labelKey ? i18n.global.t(meta.labelKey) : 'Workflow'
  return { label, dotClass: meta.dotClass, pulse: meta.pulse, edge: meta.edge }
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
    return { key, ...getStatusMeta(key) }
  })
}
