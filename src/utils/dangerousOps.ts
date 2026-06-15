import i18n from '../locales'

/**
 * 危险工具识别(FR-003)
 *
 * 规则严格按 PRD v1.0.0 FR-003 L201 闭合硬编码,
 * 不在清单内的工具/参数一律视为非危险。清单变更属新版 FR。
 *
 * Bash 危险正则:
 *   - rm -r / rm -rf
 *   - sudo
 *   - 重定向写入根目录(排除 /tmp、/var/folders、/var/tmp 三个常见临时目录)
 *   - mkfs
 *   - dd if=
 *   - fork bomb `:(){ :|:& };:`
 *
 * Write/Edit/NotebookEdit 危险路径前缀:
 *   - /etc/  /usr/  /System/  /Library/
 *   - ~/.ssh/  ~/.aws/  ~/.gnupg/
 *
 * 浏览器环境无 process.env.HOME,~ 不展开,
 * 直接对原始路径做 prefix 匹配——claude CLI 通常已展开成绝对路径,
 * `~/...` 前缀是为了应对极少数原值就带 `~` 的情况。
 */

export interface DangerousFlag {
  /** 当前只有 danger 一档 */
  level: 'danger'
  /** 一句话说明命中哪条规则,中文 */
  reason: string
}

/** Bash 命令危险正则(严格清单,不可扩展) */
const BASH_DANGER_PATTERNS: RegExp[] = [
  /\brm\s+-rf?\b/,
  /\bsudo\b/,
  />\s*\/(?!tmp|var\/folders|var\/tmp)/,
  /\bmkfs\b/,
  /\bdd\s+if=/,
  /:\(\)\{\s*:\|:&\s*\};:/,
]

/** Write/Edit/NotebookEdit 路径危险前缀(严格清单,不可扩展) */
const PATH_DANGER_PREFIXES: string[] = [
  '/etc/',
  '/usr/',
  '/System/',
  '/Library/',
  '~/.ssh/',
  '~/.aws/',
  '~/.gnupg/',
]

/**
 * 检查任何工具调用是否危险。返回 null 即非危险。
 *
 * @param toolName ContentBlock.tool_use.name
 * @param input   工具调用 input 对象
 */
export function checkDangerous(
  toolName: string,
  input: Record<string, unknown>,
): DangerousFlag | null {
  // Bash 类:input.command
  if (toolName === 'Bash') {
    const cmd = typeof input.command === 'string' ? input.command : ''
    for (const re of BASH_DANGER_PATTERNS) {
      if (re.test(cmd)) {
        return {
          level: 'danger',
          reason: i18n.global.t('danger.bashRule', { rule: re.source }),
        }
      }
    }
    return null
  }

  // Write/Edit/NotebookEdit 类:input.file_path 或 input.notebook_path
  if (toolName === 'Write' || toolName === 'Edit' || toolName === 'NotebookEdit') {
    const filePath = typeof input.file_path === 'string' ? input.file_path : ''
    const notebookPath = typeof input.notebook_path === 'string' ? input.notebook_path : ''
    const path = filePath || notebookPath
    if (!path) return null

    for (const prefix of PATH_DANGER_PREFIXES) {
      if (path.startsWith(prefix)) {
        return {
          level: 'danger',
          reason: i18n.global.t('danger.sensitivePath', { prefix }),
        }
      }
    }
    return null
  }

  return null
}
