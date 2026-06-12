/**
 * AskUserQuestion input.questions 解析（容错）
 *
 * 供只读回显（blocks/tools/ToolAskUserQuestion）与可交互问答卡（QuestionCard）共用。
 * 任何字段缺失都降级为空，不抛错。
 */

export interface ParsedOption {
  label: string
  description: string
}

export interface ParsedQuestion {
  header: string
  question: string
  multiSelect: boolean
  options: ParsedOption[]
}

/** 从工具 input 解析 questions 数组，形状异常时返回空数组 */
export function parseQuestions(input: Record<string, unknown>): ParsedQuestion[] {
  const raw = input.questions
  if (!Array.isArray(raw)) return []
  return raw.map((q): ParsedQuestion => {
    const obj = (typeof q === 'object' && q !== null ? q : {}) as Record<string, unknown>
    const options = Array.isArray(obj.options)
      ? obj.options.map((o): ParsedOption => {
          const oo = (typeof o === 'object' && o !== null ? o : {}) as Record<string, unknown>
          return {
            label: typeof oo.label === 'string' ? oo.label : '',
            description: typeof oo.description === 'string' ? oo.description : '',
          }
        })
      : []
    return {
      header: typeof obj.header === 'string' ? obj.header : '',
      question: typeof obj.question === 'string' ? obj.question : '',
      multiSelect: obj.multiSelect === true,
      options,
    }
  })
}
