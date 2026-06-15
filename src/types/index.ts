import i18n from '../locales'

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
  context_window: number | null
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

// --- 首页统计（v2.2.0，与 Rust usage_stats / probe 模块对应）---

export interface DailyUsage {
  date: string
  total: number
}

export interface ModelUsage {
  model: string
  total: number
}

export interface UsageStats {
  daily: DailyUsage[]
  month: {
    total: number
    byModel: ModelUsage[]
  }
}

/** get_schema_diagnosis 返回的子集——前端只消费诊断卡需要的字段 */
export interface SchemaDiagnosis {
  scanned_files: number
  record_types: {
    supported: Record<string, number>
    unknown: Record<string, unknown>
  }
  tools: {
    generic_undeclared: Record<string, unknown>
  }
}

// --- 工坊域(v2.3.0):get_workshop_assets 返回结构,Rust 端 #[serde(rename_all = "camelCase")] ---

export interface WorkshopSkill {
  name: string
  description: string
  version: string | null
  source: string
  path: string
}

export interface WorkshopCommand {
  name: string
  description: string
  argumentHint: string | null
  source: string
  path: string
}

export interface WorkshopAgent {
  name: string
  description: string
  source: string
  path: string
}

/** 常见三值（PRD 口径）；Rust 端对配置显式 type 原样透传，运行时可能出现其他字符串 */
export type McpTransport = 'stdio' | 'http' | 'sse' | (string & {})

export interface WorkshopMcpServer {
  name: string
  transport: McpTransport
  endpoint: string
  enabled: boolean
  source: string
  path: string
}

export interface WorkshopAssets {
  skills: WorkshopSkill[]
  commands: WorkshopCommand[]
  agents: WorkshopAgent[]
  mcpServers: WorkshopMcpServer[]
}

// --- 工具函数 ---

export function tokenTotal(t: TokenUsage): number {
  return t.input_tokens + t.output_tokens + t.cache_creation_input_tokens + t.cache_read_input_tokens
}

export function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(2)}B`
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

  if (seconds < 60) return i18n.global.t('time.justNow')
  if (minutes < 60) return i18n.global.t('time.minutesAgo', { n: minutes })
  if (hours < 24) return i18n.global.t('time.hoursAgo', { n: hours })
  if (days === 1) return i18n.global.t('time.yesterday')
  if (days < 30) return i18n.global.t('time.daysAgo', { n: days })
  return date.toLocaleDateString('zh-CN')
}

export function shortModel(model: string | null): string {
  if (!model) return ''
  // CLI 本地合成的占位消息(API Error 等),非真实模型响应
  if (model === '<synthetic>') return i18n.global.t('session.system')
  if (model.includes('fable')) return 'Fable'
  if (model.includes('opus')) return 'Opus'
  if (model.includes('sonnet-4-5') || model.includes('sonnet-4.5')) return 'Sonnet 4.5'
  if (model.includes('sonnet')) return 'Sonnet'
  if (model.includes('haiku')) return 'Haiku'
  return model
}

/** 会话显示标题（metaTitle 优先于 JSONL 原始标题） */
export function displayTitle(s: SessionSummary, metaTitle?: string): string {
  if (metaTitle) return metaTitle
  if (s.title) return s.title
  if (s.first_user_message) {
    const text = s.first_user_message.slice(0, 60)
    return text.length < s.first_user_message.length ? text + '…' : text
  }
  return i18n.global.t('session.noTitleSession')
}

/** 短 UUID（前 8 位）*/
export function shortId(id: string): string {
  return id.slice(0, 8)
}
