import { describe, it, expect } from 'vitest'
import { resolveMappedRoles } from '../../src/utils/modelEnv'

const env = {
  ANTHROPIC_DEFAULT_OPUS_MODEL: 'vendor-large-x1',
  ANTHROPIC_DEFAULT_SONNET_MODEL: 'vendor-mid-x1[1m]',
  ANTHROPIC_DEFAULT_HAIKU_MODEL: 'vendor-large-x1',
  ANTHROPIC_CUSTOM_MODEL_OPTION: 'vendor-extra-x1',
  ANTHROPIC_MODEL: 'vendor-large-x1',
}

describe('resolveMappedRoles', () => {
  it('值命中单角色槽', () => {
    expect(resolveMappedRoles('vendor-mid-x1', env)).toEqual(['SONNET'])
  })

  it('剥 [1m] 后缀再比对(双向,大小写不敏感)', () => {
    expect(resolveMappedRoles('vendor-mid-x1[1M]', env)).toEqual(['SONNET'])
    expect(resolveMappedRoles('Vendor-Mid-X1', env)).toEqual(['SONNET'])
  })

  it('同一模型映射到多角色时全部命中(声明顺序)', () => {
    expect(resolveMappedRoles('vendor-large-x1', env)).toEqual(['OPUS', 'HAIKU'])
  })

  it('裸 alias 命中对应角色(配置侧持久化值)', () => {
    expect(resolveMappedRoles('opus', env)).toEqual(['OPUS'])
  })

  it('alias 仅在该角色槽有映射时命中', () => {
    expect(resolveMappedRoles('fable', env)).toEqual([])
  })

  it('自定义第五槽与 ANTHROPIC_MODEL 兜底键不参与反查', () => {
    expect(resolveMappedRoles('vendor-extra-x1', env)).toEqual([])
  })

  it('无映射/空入参恒返回空', () => {
    expect(resolveMappedRoles('vendor-mid-x1', undefined)).toEqual([])
    expect(resolveMappedRoles('vendor-mid-x1', {})).toEqual([])
    expect(resolveMappedRoles(null, env)).toEqual([])
    expect(resolveMappedRoles('', env)).toEqual([])
  })
})
