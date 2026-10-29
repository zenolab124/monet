import type { EngineInstanceId, ProjectRef, SessionRef } from './types'

function base64Url(value: string): string {
  const bytes = new TextEncoder().encode(value)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

export function instanceKey(instance: EngineInstanceId): string {
  return `ei1.${base64Url(instance.engineId)}.${base64Url(instance.instanceId)}`
}

export function projectKey(reference: ProjectRef): string {
  return `pr1.${base64Url(reference.engine.engineId)}.${base64Url(reference.engine.instanceId)}.${base64Url(reference.nativeId)}`
}

export function sessionKey(reference: SessionRef): string {
  return `sr1.${base64Url(reference.engine.engineId)}.${base64Url(reference.engine.instanceId)}.${base64Url(reference.nativeId)}`
}

function fromBase64Url(value: string): string | null {
  try {
    const binary = atob(value.replace(/-/g, '+').replace(/_/g, '/'))
    return new TextDecoder('utf-8', { fatal: true }).decode(Uint8Array.from(binary, char => char.charCodeAt(0)))
  } catch {
    return null
  }
}

/** sessionKey 的逆运算；不是结构化会话 id 时返回 null（如本机实例的裸 uuid） */
export function parseSessionKey(key: string): SessionRef | null {
  const parts = key.split('.')
  if (parts.length !== 4 || parts[0] !== 'sr1') return null
  const [engineId, instanceId, nativeId] = parts.slice(1).map(fromBase64Url)
  if (!engineId || !instanceId || !nativeId) return null
  return { engine: { engineId, instanceId }, nativeId }
}

export function sameInstance(left: EngineInstanceId, right: EngineInstanceId): boolean {
  return left.engineId === right.engineId && left.instanceId === right.instanceId
}
