import type { ContentBlock } from '@/types'

export const INLINE_RESULT_TOOLS = new Set(['Bash', 'WebSearch'])

export interface ToolResultData {
  content: string | ContentBlock[]
  is_error: boolean
}

export function flattenResultText(content: string | ContentBlock[]): string {
  if (typeof content === 'string') return content
  return content
    .filter((b): b is Extract<ContentBlock, { type: 'text' }> => b.type === 'text')
    .map(b => b.text)
    .join('\n')
}

/**
 * 过滤流式 turn content 中已被内联工具消费的 tool_result 块。
 * 历史视图不需要此函数（tool_result 在 user 消息中，被消息过滤器排除）。
 */
export function filterConsumedResults(blocks: ContentBlock[]): ContentBlock[] {
  const inlineIds = new Set<string>()
  for (const b of blocks) {
    if (b.type === 'tool_use' && INLINE_RESULT_TOOLS.has((b as { name: string }).name)) {
      inlineIds.add((b as { id: string }).id)
    }
  }
  if (inlineIds.size === 0) return blocks
  return blocks.filter(b => {
    if (b.type === 'tool_result') {
      return !inlineIds.has((b as { tool_use_id: string }).tool_use_id)
    }
    return true
  })
}
