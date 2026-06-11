// 与 Rust 端数据模型对应的 TypeScript 类型

export interface TokenUsage {
  input_tokens: number
  output_tokens: number
  cache_creation_input_tokens: number
  cache_read_input_tokens: number
}

export interface SessionSummary {
  id: string
  title: string | null
  first_user_message: string | null
  model: string | null
  git_branch: string | null
  cwd: string | null
  version: string | null
  timestamp: string | null
  last_modified: number
  total_tokens: TokenUsage
  file_size: number
  message_count: number
}

export interface Project {
  id: string
  display_path: string
  sessions: SessionSummary[]
  session_count: number
  last_active: number | null
}

// --- 会话记录（完整对话）---

export interface UserMessage {
  role: string | null
  content: string | ContentBlock[]
}

export interface AssistantMessage {
  id: string | null
  message_type: string | null
  role: string | null
  content: ContentBlock[]
  model: string | null
  stop_reason: string | null
  usage: TokenUsage | null
}

// serde(tag = "type", rename_all = "snake_case") — 与 Rust 序列化对齐
export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; thinking: string; signature?: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> }
  | { type: 'tool_result'; tool_use_id: string; content: string | ContentBlock[]; is_error: boolean }
  | { type: 'image'; source: { source_type: string; media_type: string; data_prefix: string; data_length: number } }
  | { type: 'document'; source: { source_type: string; media_type: string }; title: string | null }
  | { type: string; [key: string]: unknown }

export type SessionRecord =
  | { type: 'user'; uuid: string | null; parent_uuid: string | null; session_id: string | null; timestamp: string | null; cwd: string | null; version: string | null; git_branch: string | null; is_sidechain: boolean | null; message: UserMessage | null }
  | { type: 'assistant'; uuid: string | null; parent_uuid: string | null; session_id: string | null; timestamp: string | null; cwd: string | null; version: string | null; git_branch: string | null; is_sidechain: boolean | null; message: AssistantMessage | null }
  // system 记录字段随 Rust SystemRecord 的 rename_all = "camelCase" 序列化
  | { type: 'system'; subtype: string | null; content: string | null; level: string | null; timestamp: string | null; uuid: string | null; error: Record<string, unknown> | null; compactMetadata: Record<string, unknown> | null; retryAttempt: number | null; maxRetries: number | null }
  | { type: 'ai_title'; session_id: string | null; ai_title: string }
  | { type: 'queue_operation'; operation: string | null; timestamp: string | null; session_id: string | null }
  | { type: 'file_history_snapshot'; message_id: string | null; is_snapshot_update: boolean | null }
  | { type: 'unknown'; raw_type: string }

// --- 工具函数 ---

export function tokenTotal(t: TokenUsage): number {
  return t.input_tokens + t.output_tokens + t.cache_creation_input_tokens + t.cache_read_input_tokens
}

export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

export function relativeTime(timestamp: string | number): string {
  const date = typeof timestamp === 'number'
    ? new Date(timestamp * 1000)
    : new Date(timestamp)
  const now = Date.now()
  const diff = now - date.getTime()
  const seconds = Math.floor(diff / 1000)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (seconds < 60) return '刚刚'
  if (minutes < 60) return `${minutes} 分钟前`
  if (hours < 24) return `${hours} 小时前`
  if (days === 1) return '昨天'
  if (days < 30) return `${days} 天前`
  return date.toLocaleDateString('zh-CN')
}

export function shortModel(model: string | null): string {
  if (!model) return ''
  // CLI 本地合成的占位消息(API Error 等),非真实模型响应
  if (model === '<synthetic>') return '系统'
  if (model.includes('fable')) return 'Fable'
  if (model.includes('opus')) return 'Opus'
  if (model.includes('sonnet-4-5') || model.includes('sonnet-4.5')) return 'Sonnet 4.5'
  if (model.includes('sonnet')) return 'Sonnet'
  if (model.includes('haiku')) return 'Haiku'
  return model
}

/** 会话显示标题 */
export function displayTitle(s: SessionSummary): string {
  if (s.title) return s.title
  if (s.first_user_message) {
    const text = s.first_user_message.slice(0, 60)
    return text.length < s.first_user_message.length ? text + '…' : text
  }
  return '无标题会话'
}

/** 短 UUID（前 8 位）*/
export function shortId(id: string): string {
  return id.slice(0, 8)
}
