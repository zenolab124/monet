import { beforeEach, describe, expect, it } from 'vitest'
import {
  LOCAL_CLAUDE_INSTANCE,
  configureUiIntegrations,
  projectUiId,
  resolveNativeInstance,
  sessionUiId,
  usesNativeSessionSurface,
} from '../../src/engines/integration'
import { parseSessionKey, projectKey, sessionKey } from '../../src/engines/identity'
import type { EngineDescriptor, EngineInstanceId } from '../../src/engines/types'

const second: EngineInstanceId = { engineId: 'claude-code', instanceId: 'second-a' }

function nativeDescriptor(instance: EngineInstanceId): EngineDescriptor {
  return {
    instance,
    displayName: 'Claude Code',
    enabled: true,
    capabilities: {} as EngineDescriptor['capabilities'],
    ui: { identity: 'native', sessionSurface: 'native', installGuideUrl: null },
  }
}

describe('native instance resolution', () => {
  it('falls back to the local Claude instance only when no instance is known', () => {
    expect(resolveNativeInstance()).toEqual(LOCAL_CLAUDE_INSTANCE)
    expect(resolveNativeInstance(null, undefined)).toEqual(LOCAL_CLAUDE_INSTANCE)
    expect(resolveNativeInstance(null, second)).toBe(second)
    expect(resolveNativeInstance(second, LOCAL_CLAUDE_INSTANCE)).toBe(second)
  })
})

describe('native UI identity across instances', () => {
  beforeEach(() => configureUiIntegrations([]))

  it('keeps bare native ids for the local instance', () => {
    configureUiIntegrations([nativeDescriptor(LOCAL_CLAUDE_INSTANCE)])
    const session = { engine: LOCAL_CLAUDE_INSTANCE, nativeId: 'abc' }
    expect(sessionUiId(session)).toBe('abc')
    expect(projectUiId(session)).toBe('abc')
  })

  it('gives later native-identity instances structured ids so equal uuids do not collide', () => {
    // 其他实例先注册也不能抢走本机实例的裸 id
    configureUiIntegrations([nativeDescriptor(second), nativeDescriptor(LOCAL_CLAUDE_INSTANCE)])
    const local = { engine: LOCAL_CLAUDE_INSTANCE, nativeId: 'same-uuid' }
    const mirrored = { engine: second, nativeId: 'same-uuid' }

    expect(sessionUiId(local)).toBe('same-uuid')
    expect(sessionUiId(mirrored)).toBe(sessionKey(mirrored))
    expect(projectUiId(mirrored)).toBe(projectKey(mirrored))
    expect(sessionUiId(mirrored)).not.toBe(sessionUiId(local))
    expect(usesNativeSessionSurface(second)).toBe(true)
  })
})

describe('structured session ids', () => {
  it('round-trips through parseSessionKey and rejects anything else', () => {
    const reference = { engine: second, nativeId: '0b5c7e2a-1111-4222-8333-444455556666' }
    expect(parseSessionKey(sessionKey(reference))).toEqual(reference)
    expect(parseSessionKey('0b5c7e2a-1111-4222-8333-444455556666')).toBeNull()
    expect(parseSessionKey(projectKey(reference))).toBeNull()
    expect(parseSessionKey('sr1.!!.x.y')).toBeNull()
  })
})
