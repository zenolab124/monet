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
}

interface ChannelListResult {
  channels: ChannelInfo[]
  sessionChain: string[]
  agentChain: string[]
}

export const OFFICIAL_CHANNEL_ID = 'official'

const channels = ref<ChannelInfo[]>([])
const sessionChain = ref<string[]>([])
const agentChain = ref<string[]>([])

export async function refreshChannels(): Promise<void> {
  try {
    const r = await invoke<ChannelListResult>('list_channels')
    channels.value = r.channels
    sessionChain.value = r.sessionChain
    agentChain.value = r.agentChain
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
  for (const id of sessionChain.value) {
    if (id === OFFICIAL_CHANNEL_ID) return null
    const ch = channels.value.find(c => c.id === id)
    if (ch?.enabled !== false) return id
  }
  return null
}

export interface SaveChannelPayload {
  id: string
  name: string
  baseUrl: string
  authToken?: string
  note?: string
}

async function saveChannel(payload: SaveChannelPayload): Promise<void> {
  await invoke('save_channel', {
    id: payload.id,
    name: payload.name,
    baseUrl: payload.baseUrl,
    authToken: payload.authToken ?? null,
    note: payload.note ?? null,
  })
  await refreshChannels()
}

async function deleteChannel(id: string): Promise<void> {
  await invoke('delete_channel', { id })
  await refreshChannels()
}

async function setChannelEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke('set_channel_enabled', { id, enabled })
  await refreshChannels()
}

async function setSessionChain(chain: string[]): Promise<void> {
  await invoke('set_session_chain', { chain })
  await refreshChannels()
}

async function setAgentChain(chain: string[]): Promise<void> {
  await invoke('set_agent_chain', { chain })
  await refreshChannels()
}

async function revealChannelsDir(): Promise<void> {
  await invoke('reveal_channels_dir')
}

export function useChannels() {
  return {
    channels,
    sessionChain,
    agentChain,
    refreshChannels,
    saveChannel,
    deleteChannel,
    setChannelEnabled,
    setSessionChain,
    setAgentChain,
    revealChannelsDir,
  }
}
