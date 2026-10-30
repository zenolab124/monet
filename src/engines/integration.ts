import type { EngineDescriptor, EngineInstanceId, ProjectRef, SessionRef } from './types'
import { projectKey, sameInstance, sessionKey } from './identity'

interface UiIdentityAdapter {
  projectId(reference: ProjectRef): string
  sessionId(reference: SessionRef): string
}

const structuredIdentity: UiIdentityAdapter = {
  projectId: projectKey,
  sessionId: sessionKey,
}
const nativeIdentity: UiIdentityAdapter = {
  projectId: reference => reference.nativeId,
  sessionId: reference => reference.nativeId,
}
const integrations = new Map<string, EngineDescriptor['ui']>()

/** 本机 Claude Code 实例。原生控制器拿不到会话所属实例时(旧草稿、未落盘会话)回退到它。 */
export const LOCAL_CLAUDE_INSTANCE: Readonly<EngineInstanceId> = Object.freeze({
  engineId: 'claude-code',
  instanceId: 'default',
})

function integrationKey(instance: EngineInstanceId): string {
  return `${instance.engineId}/${instance.instanceId}`
}

/**
 * 裸 nativeId 作界面 id 只对一个实例安全:两个实例的会话 uuid 或项目路径相同时会互相覆盖。
 * 多个实例声明 native identity 时只保留一个(本机实例优先,否则取先注册者),其余降级为结构化 id。
 */
export function configureUiIntegrations(descriptors: EngineDescriptor[]) {
  integrations.clear()
  const nativeClaimants = descriptors.filter(descriptor => descriptor.ui.identity === 'native')
  const nativeOwner = nativeClaimants.find(descriptor => sameInstance(descriptor.instance, LOCAL_CLAUDE_INSTANCE))
    ?? nativeClaimants[0]
  for (const descriptor of descriptors) {
    const ui = descriptor.ui.identity === 'native' && descriptor !== nativeOwner
      ? { ...descriptor.ui, identity: 'structured' as const }
      : descriptor.ui
    integrations.set(integrationKey(descriptor.instance), ui)
  }
}

function identityAdapter(instance: EngineInstanceId): UiIdentityAdapter {
  return integrations.get(integrationKey(instance))?.identity === 'native'
    ? nativeIdentity
    : structuredIdentity
}

export function projectUiId(reference: ProjectRef): string {
  return identityAdapter(reference.engine).projectId(reference)
}

export function sessionUiId(reference: SessionRef): string {
  return identityAdapter(reference.engine).sessionId(reference)
}

export function usesNativeSessionSurface(instance: EngineInstanceId): boolean {
  return integrations.get(integrationKey(instance))?.sessionSurface === 'native'
}

/** 原生控制器所服务的实例:按顺序取第一个已知实例,都缺省时为本机实例。 */
export function resolveNativeInstance(
  ...candidates: Array<EngineInstanceId | null | undefined>
): EngineInstanceId {
  return candidates.find((candidate): candidate is EngineInstanceId => !!candidate) ?? LOCAL_CLAUDE_INSTANCE
}
