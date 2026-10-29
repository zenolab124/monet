import { describe, expect, it } from 'vitest'
import {
  isClaudeRoleAlias,
  knownClaudeModelIds,
  pinnedClaudeModels,
  roleDisplayNames,
  type ReportedClaudeModel,
} from '../../src/utils/claudeModelCatalog'
import { MODELS, officialRoleItems } from '../../src/utils/modelContext'

// 合成数据:形状与 CLI 2.1.306 initialize 响应的 models 一致
const reported: ReportedClaudeModel[] = [
  { value: 'default', displayName: 'Default (recommended)' },
  { value: 'opus', displayName: 'Opus 5.5' },
  { value: 'fable', displayName: 'Fable 5.1' },
  { value: 'sonnet', displayName: 'Sonnet 5.5' },
  { value: 'haiku', displayName: 'Haiku 5.5' },
  { value: 'claude-haiku-4-5-20251001', displayName: 'Haiku 4.5' },
  { value: 'claude-opus-5', displayName: 'Opus 5' },
  { value: 'claude-opus-4-8', displayName: 'Opus 4.8' },
  { value: 'claude-opus-4-6', displayName: 'Opus 4.6' },
  { value: 'claude-sonnet-4-6', displayName: 'Sonnet 4.6' },
]

describe('isClaudeRoleAlias', () => {
  it('把 default 与角色别名(含 [1m] 写法)视为非具体版本', () => {
    expect(isClaudeRoleAlias('default')).toBe(true)
    expect(isClaudeRoleAlias('Opus')).toBe(true)
    expect(isClaudeRoleAlias('sonnet[1m]')).toBe(true)
    expect(isClaudeRoleAlias('claude-opus-5')).toBe(false)
  })
})

describe('roleDisplayNames', () => {
  it('取别名当前解析到的版本名', () => {
    expect(roleDisplayNames(reported)).toEqual({
      opus: 'Opus 5.5',
      fable: 'Fable 5.1',
      sonnet: 'Sonnet 5.5',
      haiku: 'Haiku 5.5',
    })
  })

  it('displayName 只是别名本身时不收录(从未握手的回退数据)', () => {
    expect(roleDisplayNames([{ value: 'opus', displayName: 'opus' }, { value: 'sonnet', displayName: '' }])).toEqual({})
  })
})

describe('pinnedClaudeModels', () => {
  it('按 CLI 顺序列出具体版本,跳过 default 与别名', () => {
    expect(pinnedClaudeModels(reported).map(model => model.id)).toEqual([
      'claude-haiku-4-5-20251001',
      'claude-opus-5',
      'claude-opus-4-8',
      'claude-opus-4-6[1m]',
      'claude-opus-4-6',
      'claude-sonnet-4-6[1m]',
      'claude-sonnet-4-6',
    ])
  })

  it('内置已知版本沿用内置条目,未知版本用 CLI 名称且容量记为未知', () => {
    const models = pinnedClaudeModels(reported)
    expect(models.find(model => model.id === 'claude-opus-4-6')?.label).toBe('Opus 4.6 · 200K')
    expect(models.find(model => model.id === 'claude-haiku-4-5-20251001')).toMatchObject({ label: 'Haiku 4.5', contextWindow: 200_000 })
    expect(models.find(model => model.id === 'claude-opus-5')).toMatchObject({ label: 'Opus 5', contextWindow: 0 })
  })

  it('没有具体版本时回退内置清单', () => {
    expect(pinnedClaudeModels([])).toBe(MODELS)
    expect(pinnedClaudeModels([{ value: 'opus', displayName: 'opus' }, { value: ' ', displayName: '' }])).toBe(MODELS)
  })

  it('重复与大小写不同的 ID 只保留一次', () => {
    const models = pinnedClaudeModels([
      { value: 'claude-opus-5', displayName: 'Opus 5' },
      { value: 'CLAUDE-OPUS-5', displayName: 'Opus 5' },
    ])
    expect(models).toHaveLength(1)
  })
})

describe('knownClaudeModelIds', () => {
  it('接受别名、内置版本与 CLI 新报出的版本,不接受 default 与未知值', () => {
    const known = knownClaudeModelIds(reported)
    expect(known.has('opus')).toBe(true)
    expect(known.has('sonnet[1m]')).toBe(true)
    expect(known.has('claude-opus-5')).toBe(true)
    expect(known.has('claude-fable-5')).toBe(true)
    expect(known.has('default')).toBe(false)
    expect(known.has('claude-opus-9')).toBe(false)
  })
})

describe('officialRoleItems', () => {
  it('有解析结果时标签带出具体版本,sonnet[1m] 按 sonnet 查找', () => {
    const items = officialRoleItems(roleDisplayNames(reported))
    expect(items.map(item => item.id)).toEqual(['fable', 'opus', 'sonnet[1m]', 'haiku'])
    expect(items[2].label).toContain('Sonnet 5.5')
    expect(items[1].label).toContain('Opus 5.5')
  })

  it('没有解析结果时沿用原角色标签', () => {
    const items = officialRoleItems()
    expect(items[1].label).not.toContain('5.5')
  })
})
