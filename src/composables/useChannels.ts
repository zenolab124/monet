import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import i18n from '../locales'

/**
 * 多渠道(profile):~/.claude/cc-space/channels/<id>.json,Rust 层单源读写。
 * 敏感值(authToken)永不回传前端——列表只有掩码;配置文件是用户可手编的活文件,
 * 模块级 ref 只是展示位,每次打开下拉/设置页都应 refreshChannels 重读。
 */

export interface ChannelInfo {
  id: string
  name: string
  note: string | null
  baseUrl: string | null
  authTokenMasked: string | null
  /** env 块中 BASE_URL/AUTH_TOKEN 之外的键名(手编高级配置的存在提示) */
  extraEnvKeys: string[]
  /** 配置文件 JSON 是否可解析 */
  valid: boolean
  isDefault: boolean
}

interface ChannelListResult {
  channels: ChannelInfo[]
  defaultChannelId: string | null
}

/** 「官方」保留 id:零注入,完全沿用用户已有登录态/环境 */
export const OFFICIAL_CHANNEL_ID = 'official'

const channels = ref<ChannelInfo[]>([])
const defaultChannelId = ref<string | null>(null)

export async function refreshChannels(): Promise<void> {
  try {
    const r = await invoke<ChannelListResult>('list_channels')
    channels.value = r.channels
    defaultChannelId.value = r.defaultChannelId
  } catch (_) {
    // 读取失败保留旧值:只影响展示,不阻塞流程
  }
}

/** 渠道显示名:官方/未配置兜底;清单外 id(文件已删)显示原 id */
export function channelDisplayName(id: string | null): string {
  if (!id || id === OFFICIAL_CHANNEL_ID) return i18n.global.t('channel.official')
  return channels.value.find(c => c.id === id)?.name ?? id
}

/**
 * 解析会话渠道选择为最终注入 id(null = 不注入,走官方/登录态)。
 * 选择语义:null = 跟随应用默认;'official' = 强制官方;其他 = 指定渠道
 */
export function resolveChannel(selected: string | null): string | null {
  if (selected === OFFICIAL_CHANNEL_ID) return null
  if (selected) return selected
  const def = defaultChannelId.value
  return def && def !== OFFICIAL_CHANNEL_ID ? def : null
}

export interface SaveChannelPayload {
  id: string
  name: string
  baseUrl: string
  /** 编辑时留空 = 保持不变 */
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

async function setDefaultChannel(id: string | null): Promise<void> {
  await invoke('set_default_channel', { id })
  await refreshChannels()
}

async function revealChannelsDir(): Promise<void> {
  await invoke('reveal_channels_dir')
}

export function useChannels() {
  return {
    channels,
    defaultChannelId,
    refreshChannels,
    saveChannel,
    deleteChannel,
    setDefaultChannel,
    revealChannelsDir,
  }
}
