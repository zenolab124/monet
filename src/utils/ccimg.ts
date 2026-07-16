import { convertFileSrc } from '@tauri-apps/api/core'
import type { InjectionKey, ComputedRef } from 'vue'

/**
 * ccimg 协议：历史区(已落盘 records)的图片按需从磁盘取二进制,不再内嵌 base64 驻留 JS 堆。
 * 协议 URL: ccimg://localhost/{project_id}/{session_id}/{record_uuid}/{img_index}
 *          子 agent 会话追加 query ?agent={agent_id}
 * 用 convertFileSrc(path, 'ccimg') 拼接以自动处理平台差异
 *   (macOS ccimg://localhost/...  vs  Windows http://ccimg.localhost/...)
 */

/** 会话级图片定位上下文(projectId/sessionId/agentId 对整个宿主实例恒定;recordUuid 逐记录传参) */
export interface ImageLocator {
  /** 编码后的项目目录名(~/.claude/projects/<encoded-cwd>) */
  projectId: string
  sessionId: string
  /** 子 agent 会话:agent-<id>.jsonl 的 agent_id;主会话为空 */
  agentId?: string
}

/** provide/inject 键:会话级图片定位上下文 */
export const IMAGE_LOCATOR: InjectionKey<ComputedRef<ImageLocator | null>> = Symbol('imageLocator')

/** 段内禁止字符(与 Rust is_safe_segment 黑名单一致):路径分隔/残留百分号/控制字符 */
const UNSAFE_CHARS = /[/\\%\x00-\x1f\x7f]/

/** 与 Rust handler 同口径的段校验:黑名单式,放行非 ASCII(项目目录名可能含中文) */
function isSafeSegment(s: string): boolean {
  return s.length > 0 && s !== '.' && s !== '..' && !UNSAFE_CHARS.test(s)
}

/**
 * 拼 ccimg 协议 URL。任一坐标缺失或非法(含路径分隔符与 ..)返回 null,交由调用方兜底。
 */
export function buildCcimgUrl(
  locator: ImageLocator,
  recordUuid: string,
  imgIndex: number,
): string | null {
  const { projectId, sessionId, agentId } = locator
  if (!isSafeSegment(projectId) || !isSafeSegment(sessionId) || !isSafeSegment(recordUuid)) {
    return null
  }
  if (agentId != null && !isSafeSegment(agentId)) return null
  // path = "{project_id}/{session_id}/{record_uuid}/{img_index}";convertFileSrc 负责平台差异与百分号编码
  const path = `${projectId}/${sessionId}/${recordUuid}/${imgIndex}`
  const base = convertFileSrc(path, 'ccimg')
  return agentId ? `${base}?agent=${agentId}` : base
}

/**
 * ccimg URL 加 ?full=1 → 原图直出（点击放大场景）。
 * 默认 ccimg URL 返回服务端缩略图（长边 800），原图只在放大时按需取。
 * data: URL（流式区内存路径）原样返回。
 */
export function withFullParam(url: string): string {
  if (!url.startsWith('ccimg:') && !url.includes('.localhost')) return url
  return url.includes('?') ? `${url}&full=1` : `${url}?full=1`
}
