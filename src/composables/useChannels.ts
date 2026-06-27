import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import i18n from '../locales'

export interface ChannelInfo {
  id: string
  name: string
  note: string | null
  baseUrl: string | null
  authTokenMasked: string | null
  extraEnvKeys: string[]
  valid: boolean
  enabled: boolean
  protocol: string
  scope: string
  agentModel: string | null
  availableModels: string[]
}

export const APPLE_FM_CHANNEL_ID = 'apple-fm'

interface ChannelListResult {
  channels: ChannelInfo[]
  defaultSessionChannel: string | null
  defaultAgentChannel: string | null
  defaultAgentModel: string | null
}

export const OFFICIAL_CHANNEL_ID = 'official'

const channels = ref<ChannelInfo[]>([])
const defaultSessionChannel = ref<string | null>(null)
const defaultAgentChannel = ref<string | null>(null)
const defaultAgentModel = ref<string | null>(null)

export async function refreshChannels(): Promise<void> {
  try {
    const r = await invoke<ChannelListResult>('list_channels')
    channels.value = r.channels
    defaultSessionChannel.value = r.defaultSessionChannel
    defaultAgentChannel.value = r.defaultAgentChannel
    defaultAgentModel.value = r.defaultAgentModel
  } catch {
    // 读取失败保留旧值
  }
}

export function channelDisplayName(id: string | null): string {
  if (!id || id === OFFICIAL_CHANNEL_ID) return i18n.global.t('channel.official')
  return channels.value.find(c => c.id === id)?.name ?? id
}

export function resolveChannel(selected: string | null): string | null {
  if (selected === OFFICIAL_CHANNEL_ID) return null
  if (selected) return selected
  return defaultSessionChannel.value ?? null
}

export interface SaveChannelPayload {
  id: string
  name: string
  baseUrl: string
  authToken?: string
  note?: string
  protocol?: string
  scope?: string
  agentModel?: string
  availableModels?: string[]
}

async function saveChannel(payload: SaveChannelPayload): Promise<void> {
  await invoke('save_channel', {
    id: payload.id,
    name: payload.name,
    baseUrl: payload.baseUrl,
    authToken: payload.authToken ?? null,
    note: payload.note ?? null,
    protocol: payload.protocol ?? null,
    scope: payload.scope ?? null,
    agentModel: payload.agentModel ?? null,
    availableModels: payload.availableModels ?? null,
  })
  await refreshChannels()
}

export interface AgentFeaturePrefs {
  preferredChannel: string | null
  preferredModel: string | null
}

const agentPreferences = ref<Record<string, AgentFeaturePrefs>>({})

async function loadAgentPreferences(): Promise<void> {
  try {
    agentPreferences.value = await invoke<Record<string, AgentFeaturePrefs>>('get_agent_preferences')
  } catch {
    // ignore
  }
}

async function setDefaultSessionChannel(id: string | null): Promise<void> {
  await invoke('set_default_session_channel', { id })
  defaultSessionChannel.value = id
}

async function setDefaultAgentModel(channel: string | null, model: string | null): Promise<void> {
  await invoke('set_default_agent_model', { channel, model })
  defaultAgentChannel.value = channel
  defaultAgentModel.value = model
}

async function setAgentFeatureModel(key: string, channel: string | null, model: string | null): Promise<void> {
  await invoke('set_agent_feature_model', { key, channel, model })
  agentPreferences.value = {
    ...agentPreferences.value,
    [key]: { preferredChannel: channel, preferredModel: model },
  }
}

async function deleteChannel(id: string): Promise<void> {
  await invoke('delete_channel', { id })
  await refreshChannels()
}

async function setChannelEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke('set_channel_enabled', { id, enabled })
  await refreshChannels()
}

const revealedTokens = ref<Record<string, string>>({})

async function revealToken(id: string): Promise<string | null> {
  if (revealedTokens.value[id]) return revealedTokens.value[id]
  try {
    const token = await invoke<string | null>('get_channel_token', { id })
    if (token) revealedTokens.value = { ...revealedTokens.value, [id]: token }
    return token
  } catch { return null }
}

function hideToken(id: string) {
  const { [id]: _, ...rest } = revealedTokens.value
  revealedTokens.value = rest
}

async function revealChannelsDir(): Promise<void> {
  await invoke('reveal_channels_dir')
}

interface ProbeResult {
  online: boolean
  status: string
  models: string[]
  latencyMs: number
}

const probeResults = ref<Record<string, ProbeResult>>({})
const probing = ref<Record<string, boolean>>({})

async function probeChannel(id: string): Promise<ProbeResult | null> {
  probing.value = { ...probing.value, [id]: true }
  try {
    const result = await invoke<ProbeResult>('probe_channel', { id })
    probeResults.value = { ...probeResults.value, [id]: result }
    return result
  } catch {
    return null
  } finally {
    probing.value = { ...probing.value, [id]: false }
  }
}

async function probeAllChannels(): Promise<void> {
  const ids = channels.value.map(c => c.id)
  await Promise.allSettled(ids.map(id => probeChannel(id)))
}

export function useChannels() {
  return {
    channels,
    defaultSessionChannel,
    defaultAgentChannel,
    defaultAgentModel,
    probeResults,
    probing,
    agentPreferences,
    refreshChannels,
    saveChannel,
    deleteChannel,
    setChannelEnabled,
    setDefaultSessionChannel,
    setDefaultAgentModel,
    setAgentFeatureModel,
    revealChannelsDir,
    probeChannel,
    probeAllChannels,
    revealedTokens,
    revealToken,
    hideToken,
    loadAgentPreferences,
  }
}
