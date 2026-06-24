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
import i18n from '../locales'

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

/** 命令清单：native = 前端处理，pass = 透传 CLI */
export function getSlashCommands(): SlashCommand[] {
  const t = (k: string) => i18n.global.t(k)
  return [
    { name: 'new',       hint: t('slash.hintNew'),       hasArg: false, category: 'native' },
    { name: 'clear',     hint: t('slash.hintClear'),     hasArg: false, category: 'native' },
    { name: 'cd',        hint: t('slash.hintCd'),        hasArg: true, argHint: '<path>', category: 'native' },
    { name: 'help',      hint: t('slash.hintHelp'),      hasArg: false, category: 'native' },
    { name: 'model',     hint: t('slash.hintModel'),     hasArg: true, argHint: '<name>', category: 'pass' },
    { name: 'compact',   hint: t('slash.hintCompact'),   hasArg: false, category: 'pass' },
    { name: 'config',    hint: t('slash.hintConfig'),    hasArg: false, category: 'pass' },
    { name: 'cost',      hint: t('slash.hintCost'),      hasArg: false, category: 'pass' },
    { name: 'diff',      hint: t('slash.hintDiff'),      hasArg: false, category: 'pass' },
    { name: 'doctor',    hint: t('slash.hintDoctor'),    hasArg: false, category: 'pass' },
    { name: 'effort',    hint: t('slash.hintEffort'),    hasArg: true, argHint: '<level>', category: 'pass' },
    { name: 'fast',      hint: t('slash.hintFast'),      hasArg: false, category: 'pass' },
    { name: 'hooks',     hint: t('slash.hintHooks'),     hasArg: false, category: 'pass' },
    { name: 'init',      hint: t('slash.hintInit'),      hasArg: false, category: 'pass' },
    { name: 'login',     hint: t('slash.hintLogin'),     hasArg: false, category: 'pass' },
    { name: 'logout',    hint: t('slash.hintLogout'),    hasArg: false, category: 'pass' },
    { name: 'mcp',       hint: t('slash.hintMcp'),       hasArg: false, category: 'pass' },
    { name: 'memory',    hint: t('slash.hintMemory'),    hasArg: false, category: 'pass' },
    { name: 'permissions', hint: t('slash.hintPermissions'), hasArg: false, category: 'pass' },
    { name: 'review',    hint: t('slash.hintReview'),    hasArg: false, category: 'pass' },
    { name: 'stats',     hint: t('slash.hintStats'),     hasArg: false, category: 'pass' },
    { name: 'status',    hint: t('slash.hintStatus'),    hasArg: false, category: 'pass' },
    { name: 'terminal-setup', hint: t('slash.hintTerminalSetup'), hasArg: false, category: 'pass' },
    { name: 'theme',     hint: t('slash.hintTheme'),     hasArg: true, argHint: '<name>', category: 'pass' },
    { name: 'undo',      hint: t('slash.hintUndo'),      hasArg: false, category: 'pass' },
    { name: 'vim',       hint: t('slash.hintVim'),       hasArg: false, category: 'pass' },
    { name: 'add-dir',   hint: t('slash.hintAddDir'),    hasArg: true, argHint: '<path>', category: 'pass' },
    { name: 'bug',       hint: t('slash.hintBug'),       hasArg: false, category: 'pass' },
    { name: 'ide',       hint: t('slash.hintIde'),       hasArg: false, category: 'pass' },
    { name: 'resume',    hint: t('slash.hintResume'),    hasArg: false, category: 'pass' },
  ]
}

export const SLASH_COMMANDS: SlashCommand[] = getSlashCommands()

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

/** 严格前缀过滤，不做模糊匹配 */
export function filterCommands(input: string): SlashCommand[] {
  if (!input.startsWith('/')) return []
  const prefix = input.slice(1).toLowerCase()
  const commands = getSlashCommands()
  if (prefix === '') return commands
  return commands.filter((c) => c.name.startsWith(prefix))
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

  const cmd = getSlashCommands().find((c) => c.name === name)
  if (!cmd) {
    return { kind: 'unknown', raw }
  }

  // 命中命令但需要参数校验
  if (cmd.name === 'model') {
    if (!arg) {
      return {
        kind: 'invalid',
        cmd,
        reason: i18n.global.t('slash.errorModelRequired'),
      }
    }
    if (!KNOWN_MODELS.has(arg.toLowerCase())) {
      return {
        kind: 'invalid',
        cmd,
        reason: i18n.global.t('slash.errorModelUnknown'),
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
        reason: i18n.global.t('slash.errorCdRequired'),
      }
    }
    return { kind: 'native', cmd, arg }
  }

  // 其余 native 无参命令
  return { kind: cmd.category, cmd, arg }
}
