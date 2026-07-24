/**
 * 轻量 SGR ANSI 转义序列解析器
 * 只关心颜色/粗体，其余 CSI / OSC 序列一律剥离
 */

export interface AnsiSpan {
  text: string
  colorClass: string
}

// SGR 码 → CSS class 映射（black/white 归入默认）
const SGR_CLASS: Record<number, string> = {
  31: 'ansi-red', 91: 'ansi-red',
  32: 'ansi-green', 92: 'ansi-green',
  33: 'ansi-yellow', 93: 'ansi-yellow',
  34: 'ansi-blue', 94: 'ansi-blue',
  35: 'ansi-magenta', 95: 'ansi-magenta',
  36: 'ansi-blue', 96: 'ansi-blue', // cyan → blue（设计规范）
}

// 256 色前 8 色映射到标准前景码
const BASE8_TO_SGR: Record<number, number> = {
  0: 30, 1: 31, 2: 32, 3: 33, 4: 34, 5: 35, 6: 36, 7: 37,
}

/** 将 256 色号钳位到 0-7 基础色 */
function clamp256(n: number): number {
  if (n < 8) return n
  if (n < 16) return n - 8 // 亮色 8-15 → 0-7
  if (n >= 232) {
    // 灰阶 232-255：暗半归黑(0)，亮半归白(7)
    return n < 244 ? 0 : 7
  }
  // 216 色立方 16-231：取最大分量对应的基础色（简化）
  const idx = n - 16
  const r = Math.floor(idx / 36)
  const g = Math.floor((idx % 36) / 6)
  const b = idx % 6
  const max = Math.max(r, g, b)
  if (max === 0) return 0
  if (r === max && g === max) return 3 // yellow
  if (r === max) return 1 // red
  if (g === max) return 2 // green
  if (b === max) return 4 // blue
  return 7
}

/**
 * 匹配所有 ANSI 转义序列：
 * - CSI：\x1b[ ... (字母结尾)
 * - OSC：\x1b] ... (ST 或 BEL 结尾)
 */
const RE_ANSI = /\x1b\[[0-9;]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g

/** 剥离所有 ANSI 转义序列，返回纯文本 */
export function stripAnsi(text: string): string {
  return text.replace(RE_ANSI, '')
}

/** 解析 SGR ANSI 序列，返回带颜色 class 的文本片段 */
export function parseAnsi(raw: string): AnsiSpan[] {
  const spans: AnsiSpan[] = []
  let colorClass = ''
  let lastIndex = 0

  // 逐段扫描：转义序列之间的文本作为 span
  for (const m of raw.matchAll(RE_ANSI)) {
    // 转义序列前的文本
    if (m.index > lastIndex) {
      pushSpan(spans, raw.slice(lastIndex, m.index), colorClass)
    }
    lastIndex = m.index + m[0].length

    // 只处理 SGR（以 m 结尾的 CSI）
    const seq = m[0]
    if (!seq.startsWith('\x1b[') || !seq.endsWith('m')) continue

    const codes = seq.slice(2, -1).split(';').map(Number)
    for (let i = 0; i < codes.length; i++) {
      const c = codes[i]
      if (c === 0 || c === 39) {
        colorClass = '' // 重置 / 默认前景
      } else if (c === 1 || c === 22) {
        // 粗体开/关：Paper 主题不区分，忽略
      } else if (c === 38 && codes[i + 1] === 5 && i + 2 < codes.length) {
        // 256 色：38;5;N
        const base = clamp256(codes[i + 2])
        const sgr = BASE8_TO_SGR[base]
        colorClass = (sgr !== undefined ? SGR_CLASS[sgr] : undefined) ?? ''
        i += 2
      } else if (SGR_CLASS[c] !== undefined) {
        colorClass = SGR_CLASS[c]
      } else if ((c >= 30 && c <= 37) || (c >= 90 && c <= 97)) {
        // black(30/90) / white(37/97) 走默认
        colorClass = ''
      }
    }
  }

  // 尾部剩余文本
  if (lastIndex < raw.length) {
    pushSpan(spans, raw.slice(lastIndex), colorClass)
  }
  return spans
}

/** 追加 span，同色合并，空文本跳过 */
function pushSpan(spans: AnsiSpan[], text: string, colorClass: string): void {
  if (!text) return
  const last = spans[spans.length - 1]
  if (last && last.colorClass === colorClass) {
    last.text += text
  } else {
    spans.push({ text, colorClass })
  }
}
