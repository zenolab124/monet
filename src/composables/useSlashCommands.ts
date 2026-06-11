/**
 * 斜杠命令补全（FR-004）
 *
 * 功能：
 * - 维护本版可用的命令清单（5 条：new / clear / cd / help / model）
 * - 提供「是否触发面板」的严格判定
 * - 提供前缀过滤
 * - 提供把输入字符串解析为 ParsedCommand 的能力
 *
 * 触发条件（PRD L266）：
 *   `/` 在行首，且后无其它字符或后跟字母/连字符；
 *   即：从位置 0 到 cursorPos 的子串完全匹配 `^\/[a-z\-]*$`。
 */

import { MODELS, MODEL_ALIASES } from '@/utils/modelContext'

/** 命令分类 */
export type SlashCommandCategory = 'native' | 'pass'

/** 单条命令的元数据 */
export interface SlashCommand {
  /** 命令名（不含前导 /），如 "new" */
  name: string
  /** 一句话说明（中文） */
  hint: string
  /** 是否需要参数（cd / model 为 true） */
  hasArg: boolean
  /** 参数提示，如 "<path>" */
  argHint?: string
  /** 分类：native = 前端原生处理；pass = 透传/参数命令 */
  category: SlashCommandCategory
}

/** 本版命令清单（仅 5 条，PRD L234-243） */
export const SLASH_COMMANDS: SlashCommand[] = [
  {
    name: 'new',
    hint: '在当前项目目录开新会话（新建一个 pane，不 resume）',
    hasArg: false,
    category: 'native',
  },
  {
    name: 'clear',
    hint: '清空当前 pane 的流式渲染区（不影响磁盘 jsonl）',
    hasArg: false,
    category: 'native',
  },
  {
    name: 'cd',
    hint: '跳转到另一个项目目录的会话列表',
    hasArg: true,
    argHint: '<path>',
    category: 'native',
  },
  {
    name: 'help',
    hint: '在当前 pane 渲染本地帮助卡片',
    hasArg: false,
    category: 'native',
  },
  {
    name: 'model',
    hint: '切换当前会话的模型（fable / opus / sonnet / haiku 或完整 ID）',
    hasArg: true,
    argHint: '<name>',
    category: 'pass',
  },
]

/** 合法 /model 参数 = 模型清单完整 ID + 短别名（单源派生自 modelContext） */
const KNOWN_MODELS = new Set([
  ...MODELS.map(m => m.id),
  ...Object.keys(MODEL_ALIASES),
])

/** 短别名展开为完整 ID（已是完整 ID 则原样返回） */
function resolveModelArg(arg: string): string {
  return MODEL_ALIASES[arg] ?? arg
}

/** 触发面板的严格正则：从 0 到 cursorPos 必须完全匹配 */
const TRIGGER_RE = /^\/[a-z-]*$/

/**
 * 是否应该弹出补全面板。
 *
 * 规则：cursor 之前的内容是 `/` 或 `/xxx`（无空格、无其它字符，且 `/` 在文本起始位置 0）。
 * 即：从位置 0 到 cursorPos 的子串完全匹配 `^/[a-z\-]*$`。
 */
export function shouldTriggerPanel(input: string, cursorPos: number): boolean {
  if (cursorPos < 1) return false
  if (cursorPos > input.length) return false
  const slice = input.slice(0, cursorPos)
  return TRIGGER_RE.test(slice)
}

/**
 * 严格前缀过滤。
 *
 * - "/" 返回全部 5 条
 * - "/h" 返回 [help]
 * - "/x" 返回空数组
 *
 * 不做模糊匹配。输入若不以 `/` 开头，统一返回空数组。
 */
export function filterCommands(input: string): SlashCommand[] {
  if (!input.startsWith('/')) return []
  const prefix = input.slice(1).toLowerCase()
  if (prefix === '') return [...SLASH_COMMANDS]
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(prefix))
}

/** 解析结果 */
export type ParsedCommand =
  | { kind: 'unknown'; raw: string }
  | { kind: 'native'; cmd: SlashCommand; arg: string }
  | { kind: 'pass'; cmd: SlashCommand; arg: string }
  | { kind: 'invalid'; cmd: SlashCommand; reason: string }

/**
 * 把输入字符串解析为 ParsedCommand。
 *
 * - 不以 `/` 开头：unknown
 * - 命令名不在清单：unknown（按普通文本发送）
 * - `/model` 但参数不在 sonnet/opus/haiku：invalid
 * - native / pass：返回对应分类
 */
export function parseCommand(input: string): ParsedCommand {
  const raw = input
  const trimmed = input.trim()

  if (!trimmed.startsWith('/')) {
    return { kind: 'unknown', raw }
  }

  // 第一个空白前是命令名，剩余是参数
  const body = trimmed.slice(1)
  const spaceIdx = body.search(/\s/)
  const name = (spaceIdx === -1 ? body : body.slice(0, spaceIdx)).toLowerCase()
  const arg = spaceIdx === -1 ? '' : body.slice(spaceIdx + 1).trim()

  const cmd = SLASH_COMMANDS.find((c) => c.name === name)
  if (!cmd) {
    return { kind: 'unknown', raw }
  }

  // 命中命令但需要参数校验
  if (cmd.name === 'model') {
    if (!arg) {
      return {
        kind: 'invalid',
        cmd,
        reason: '请提供模型名，可选 fable / opus / sonnet / haiku 或完整 ID',
      }
    }
    if (!KNOWN_MODELS.has(arg.toLowerCase())) {
      return {
        kind: 'invalid',
        cmd,
        reason: '未知模型，可选 fable / opus / sonnet / haiku 或完整 ID',
      }
    }
    // 短别名展开为完整 ID 再持久化:UI 选中态与 --model 传参统一用完整 ID
    return { kind: 'pass', cmd, arg: resolveModelArg(arg.toLowerCase()) }
  }

  if (cmd.name === 'cd') {
    if (!arg) {
      return {
        kind: 'invalid',
        cmd,
        reason: '请提供目标项目路径',
      }
    }
    return { kind: 'native', cmd, arg }
  }

  // 其余 native 无参命令
  return { kind: cmd.category, cmd, arg }
}
