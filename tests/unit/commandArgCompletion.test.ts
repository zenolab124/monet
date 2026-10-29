import { describe, expect, it } from 'vitest'
import {
  argumentCompletion,
  filterArgOptions,
  parseCommand,
  type CommandArgOption,
  type CommandCatalogContext,
} from '../../src/composables/useSlashCommands'

const models: CommandArgOption[] = [
  { value: 'opus', label: 'Opus 5.5 · Latest' },
  { value: 'sonnet[1m]', label: 'Sonnet 5.5 · Latest' },
  { value: 'claude-opus-5', label: 'Opus 5', secondary: true },
  { value: 'claude-sonnet-5', label: 'Sonnet 5', secondary: true },
]

const claude: CommandCatalogContext = {
  engineId: 'claude-code',
  argOptions: { model: models, effort: [{ value: 'high', label: 'High' }] },
}

function complete(input: string, context = claude, cursor = input.length) {
  return argumentCompletion(input, cursor, [], [], context)
}

describe('filterArgOptions', () => {
  it('空查询返回全部候选并保持顺序', () => {
    expect(filterArgOptions(models, '').map(option => option.value)).toEqual(models.map(option => option.value))
  })

  it('值或标签前缀命中排在包含命中之前', () => {
    expect(filterArgOptions(models, 'son').map(option => option.value)).toEqual(['sonnet[1m]', 'claude-sonnet-5'])
    expect(filterArgOptions(models, 'opus 5').map(option => option.value)).toEqual(['opus', 'claude-opus-5'])
  })

  it('不区分大小写，无命中时为空', () => {
    expect(filterArgOptions(models, 'SONNET 5').map(option => option.value)).toEqual(['sonnet[1m]', 'claude-sonnet-5'])
    expect(filterArgOptions(models, 'gpt')).toEqual([])
  })
})

describe('argumentCompletion', () => {
  it('命令名后输入空格即进入参数候选', () => {
    const result = complete('/model ')
    expect(result?.command.name).toBe('model')
    expect(result?.prefix).toBe('/')
    expect(result?.options).toHaveLength(4)
  })

  it('按已输入的参数过滤', () => {
    expect(complete('/model claude-')?.options.map(option => option.value)).toEqual(['claude-opus-5', 'claude-sonnet-5'])
    expect(complete('/effort h')?.options.map(option => option.value)).toEqual(['high'])
  })

  it('无候选的命令、命令名未输完、光标不在末尾或参数含空格时不触发', () => {
    expect(complete('/chrome ')).toBeNull()
    expect(complete('/model')).toBeNull()
    expect(complete('/model opus', claude, 3)).toBeNull()
    expect(complete('/model opus extra')).toBeNull()
    expect(complete('/model  ')).toBeNull()
  })

  it('上下文没有提供候选时不触发', () => {
    expect(complete('/model ', { engineId: 'claude-code' })).toBeNull()
  })

  it('无匹配时仍返回空候选，由面板提示按原文发送', () => {
    expect(complete('/model gpt')?.options).toEqual([])
  })
})

describe('parseCommand 与候选', () => {
  it('当前渠道的候选按原样生效（保留大小写）', () => {
    const context: CommandCatalogContext = {
      engineId: 'claude-code',
      argOptions: { model: [{ value: 'DeepSeek-V4-Pro', label: 'DeepSeek V4 Pro' }] },
    }
    const parsed = parseCommand('/model deepseek-v4-pro', [], [], context)
    expect(parsed).toMatchObject({ kind: 'pass', arg: 'DeepSeek-V4-Pro' })
  })

  it('候选之外的未知模型仍被拦下', () => {
    expect(parseCommand('/model claude-opus-9', [], [], claude).kind).toBe('invalid')
  })
})
