/**
 * 斜杠命令补全（FR-004）
 *
 * 三类命令来源：
 * 1. builtin — 硬编码的内置命令（/new, /clear, /help, /model, /compact …）
 * 2. skill  — 从 ~/.claude/skills/ 和项目 .claude/skills/ 动态扫描
 * 3. command — 从 ~/.claude/commands/ 和项目 .claude/commands/ 动态扫描
 *
 * Skills 和 commands 统一透传 CLI，用户体验与 CLI 原生 / 补全一致。
 */

import { MODELS, MODEL_ALIASES } from '@/utils/modelContext'
import type { WorkshopSkill, WorkshopCommand } from '@/types'
import i18n from '../locales'

/** 命令分类 */
export type SlashCommandCategory = 'native' | 'pass' | 'skill' | 'command'

/** 单条命令的元数据 */
export interface SlashCommand {
  name: string
  hint: string
  hasArg: boolean
  argHint?: string
  category: SlashCommandCategory
}

/** 内置命令清单 */
export function getBuiltinCommands(): SlashCommand[] {
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

/** @deprecated 使用 getBuiltinCommands + dynamic 合并 */
export function getSlashCommands(): SlashCommand[] {
  return getBuiltinCommands()
}

export const SLASH_COMMANDS: SlashCommand[] = getBuiltinCommands()

/** WorkshopSkill → SlashCommand */
function skillToSlash(s: WorkshopSkill): SlashCommand {
  const hint = s.description
    ? (s.description.length > 60 ? s.description.slice(0, 57) + '…' : s.description)
    : ''
  return {
    name: s.name,
    hint,
    hasArg: !!s.argumentHint,
    argHint: s.argumentHint ?? undefined,
    category: 'skill',
  }
}

/** WorkshopCommand → SlashCommand */
function commandToSlash(c: WorkshopCommand): SlashCommand {
  const hint = c.description
    ? (c.description.length > 60 ? c.description.slice(0, 57) + '…' : c.description)
    : ''
  return {
    name: c.name,
    hint,
    hasArg: !!c.argumentHint,
    argHint: c.argumentHint ?? undefined,
    category: 'command',
  }
}

/** 合并三类来源，去重（内置优先） */
export function getAllCommands(
  skills?: WorkshopSkill[],
  commands?: WorkshopCommand[],
): SlashCommand[] {
  const builtins = getBuiltinCommands()
  const seen = new Set(builtins.map(c => c.name))
  const result = [...builtins]
  for (const s of skills ?? []) {
    if (!seen.has(s.name)) {
      seen.add(s.name)
      result.push(skillToSlash(s))
    }
  }
  for (const c of commands ?? []) {
    if (!seen.has(c.name)) {
      seen.add(c.name)
      result.push(commandToSlash(c))
    }
  }
  return result
}

/** 合法 /model 参数 = 模型清单完整 ID + 短别名（单源派生自 modelContext） */
const KNOWN_MODELS = new Set([
  ...MODELS.map(m => m.id),
  ...Object.keys(MODEL_ALIASES),
])

function resolveModelArg(arg: string): string {
  return MODEL_ALIASES[arg] ?? arg
}

const TRIGGER_RE = /^\/[a-z-]*$/

export function shouldTriggerPanel(input: string, cursorPos: number): boolean {
  if (cursorPos < 1) return false
  if (cursorPos > input.length) return false
  const slice = input.slice(0, cursorPos)
  return TRIGGER_RE.test(slice)
}

/** 严格前缀过滤：接受可选的动态源 */
export function filterCommands(
  input: string,
  skills?: WorkshopSkill[],
  commands?: WorkshopCommand[],
): SlashCommand[] {
  if (!input.startsWith('/')) return []
  const prefix = input.slice(1).toLowerCase()
  const all = getAllCommands(skills, commands)
  if (prefix === '') return all
  return all.filter((c) => c.name.startsWith(prefix))
}

export type ParsedCommand =
  | { kind: 'unknown'; raw: string }
  | { kind: 'native'; cmd: SlashCommand; arg: string }
  | { kind: 'pass'; cmd: SlashCommand; arg: string }
  | { kind: 'invalid'; cmd: SlashCommand; reason: string }

export function parseCommand(
  input: string,
  skills?: WorkshopSkill[],
  commands?: WorkshopCommand[],
): ParsedCommand {
  const raw = input
  const trimmed = input.trim()

  if (!trimmed.startsWith('/')) {
    return { kind: 'unknown', raw }
  }

  const body = trimmed.slice(1)
  const spaceIdx = body.search(/\s/)
  const name = (spaceIdx === -1 ? body : body.slice(0, spaceIdx)).toLowerCase()
  const arg = spaceIdx === -1 ? '' : body.slice(spaceIdx + 1).trim()

  const all = getAllCommands(skills, commands)
  const cmd = all.find((c) => c.name === name)
  if (!cmd) {
    return { kind: 'unknown', raw }
  }

  if (cmd.name === 'model') {
    if (!arg) {
      return { kind: 'invalid', cmd, reason: i18n.global.t('slash.errorModelRequired') }
    }
    if (!KNOWN_MODELS.has(arg.toLowerCase())) {
      return { kind: 'invalid', cmd, reason: i18n.global.t('slash.errorModelUnknown') }
    }
    return { kind: 'pass', cmd, arg: resolveModelArg(arg.toLowerCase()) }
  }

  if (cmd.name === 'cd') {
    if (!arg) {
      return { kind: 'invalid', cmd, reason: i18n.global.t('slash.errorCdRequired') }
    }
    return { kind: 'native', cmd, arg }
  }

  // skill/command 一律透传
  if (cmd.category === 'skill' || cmd.category === 'command') {
    return { kind: 'pass', cmd, arg }
  }

  return { kind: cmd.category, cmd, arg }
}
