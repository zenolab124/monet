import i18n from '../locales'

/** 远端连接断开后的错误前缀：队列暂停自动发送，等用户手动重发（断线不自动重连） */
export const REMOTE_DISCONNECTED_PREFIX = 'REMOTE_DISCONNECTED:'

/** 流式错误的展示文案：远端启动与断线的错误码换成本地化文案，其余原样返回 */
export function describeStreamError(error: string): string {
  const t = i18n.global.t
  if (error.startsWith(REMOTE_DISCONNECTED_PREFIX)) {
    return t('remote.disconnected', { alias: error.slice(REMOTE_DISCONNECTED_PREFIX.length) })
  }
  const separator = error.indexOf(':')
  const code = separator >= 0 ? error.slice(0, separator) : error
  if (!code.startsWith('REMOTE_')) return error
  const key = `remote.launchError.${code}`
  const text = t(key, { detail: separator >= 0 ? error.slice(separator + 1) : '' })
  return text === key ? error : text
}
