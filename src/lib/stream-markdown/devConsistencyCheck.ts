/**
 * 开发期一致性校验(PRD v2.5.0 FR-006):「段拼接 HTML」与「全文一次渲染 HTML」的语义等价断言。
 * 仅 DEV 构建生效,生产零执行零开销。不一致样本 console.warn 落档——
 * 该数据兼做「渐进天花板」探测器(设计稿临界信号①:不一致率长期 >5% 触发单容器重做评估)。
 *
 * 归一化规则(比对语义等价,不是字节相等):
 * 1. 剥掉 shiki 上色标签:所有 <span ...> 与 </span> 移除,保留文本——素/彩两版的差异全在 span 层
 * 2. 剥掉 code 块 wrapper 的复制按钮(含 SVG,由 wrapCodeBlocks 注入,两侧同构但保险起见一并剥)
 * 3. 折叠连续空白为单空格,去除标签间空白——markdown-it 对块边界的换行输出在分段渲染下可能不同
 */

export function normalizeHtml(html: string): string {
  return html
    .replace(/<button class="code-copy-btn"[\s\S]*?<\/button>/g, '')
    .replace(/<\/?span[^>]*>/g, '')
    .replace(/>\s+</g, '><')
    .replace(/\s+/g, ' ')
    .trim()
}

/** djb2 短 hash,只用于落档标识,无安全语义 */
function hashStr(s: string): string {
  let h = 5381
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0
  return (h >>> 0).toString(16)
}

let checkTotal = 0
let checkFailed = 0

/** 段拼接 vs 全文渲染 一致性自查;失败落档并累计比率 */
export function devCheckSegments(sourceText: string, joinedHtml: string, fullHtml: string): void {
  try {
    checkTotal++
    const a = normalizeHtml(joinedHtml)
    const b = normalizeHtml(fullHtml)
    if (a === b) return
    checkFailed++
    // 定位首个分歧点,方便直接看到问题构造
    let i = 0
    while (i < a.length && i < b.length && a[i] === b[i]) i++
    console.warn('[stream-consistency] 段拼接 ≠ 全文渲染', {
      rate: `${checkFailed}/${checkTotal}`,
      sourceHead: sourceText.slice(0, 200),
      sourceChars: sourceText.length,
      hashJoined: hashStr(a),
      hashFull: hashStr(b),
      divergeAt: i,
      joinedCtx: a.slice(Math.max(0, i - 60), i + 60),
      fullCtx: b.slice(Math.max(0, i - 60), i + 60),
    })
  } catch {
    // 校验器自身异常绝不影响主流程
  }
}

/** 换树前后容器高度对比:diff 超阈值落档(换树位移的直接观测) */
export function devCheckSwapHeight(label: string, before: number, after: number): void {
  try {
    const diff = Math.abs(after - before)
    if (diff <= 1) return
    console.warn('[stream-consistency] 换树高度位移', { label, before, after, diff })
  } catch { /* 同上 */ }
}
