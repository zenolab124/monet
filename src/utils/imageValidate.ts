import i18n from '../locales'

/**
 * 图片校验工具:MIME + magic bytes 双层校验。
 * PRD FR-005 校验依据:不靠扩展名(扩展名可伪造,粘贴板图片无扩展名)。
 *
 * 接受:png / jpg / jpeg / gif / webp
 * 拒绝:其它一切(svg / heic / bmp / tiff / 视频 / 音频 / PDF 等)
 */

/** 允许的 MIME 集合 */
export const ALLOWED_IMAGE_MIMES = [
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
] as const

export type AllowedMime = (typeof ALLOWED_IMAGE_MIMES)[number]

/** 为 magic bytes 检查保留前 12 字节 */
const MAGIC_PROBE_BYTES = 12

/** 校验结果 */
export interface ValidateResult {
  ok: boolean
  /** 推断出的 MIME(成功时);失败时为 null */
  mime: AllowedMime | null
  /** 失败原因(中文,可直接展示) */
  reason: string | null
}

/**
 * 综合校验入口:优先用 MIME,MIME 不可信时(空 / application/octet-stream / 不在白名单)
 * 退回 magic bytes 检查。两层都不通过 → 拒绝。
 *
 * @param hintedMime  来源声明的 MIME(粘贴板:ClipboardItem.types,拖拽:File.type)
 * @param data        文件二进制内容(用于 magic bytes 检查)
 */
export function validateImage(
  hintedMime: string,
  data: ArrayBuffer | Uint8Array,
): ValidateResult {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data)

  // 第一步:MIME 命中白名单则直接通过(主路径)
  if (isAllowedMime(hintedMime)) {
    // 加固:仍做一次 magic bytes 校验,防止 MIME 与实际内容不符
    const magic = detectByMagic(bytes)
    if (magic && magic === hintedMime) {
      return { ok: true, mime: magic, reason: null }
    }
    // MIME 与 magic 都白名单成员但不一致(罕见):以 magic 为准
    if (magic) return { ok: true, mime: magic, reason: null }
    return {
      ok: false,
      mime: null,
      reason: i18n.global.t('image.mimeHeaderMismatch'),
    }
  }

  // 第二步:MIME 不可信(空 / octet-stream / 在拒绝名单),退回 magic bytes
  const magic = detectByMagic(bytes)
  if (magic) {
    return { ok: true, mime: magic, reason: null }
  }

  // 失败:给出拒绝原因
  if (hintedMime === 'image/svg+xml') {
    return { ok: false, mime: null, reason: i18n.global.t('image.noSvg') }
  }
  if (hintedMime.startsWith('video/')) {
    return { ok: false, mime: null, reason: i18n.global.t('image.imageOnly') }
  }
  if (hintedMime.startsWith('audio/')) {
    return { ok: false, mime: null, reason: i18n.global.t('image.imageOnly') }
  }
  if (hintedMime === 'application/pdf') {
    return { ok: false, mime: null, reason: i18n.global.t('image.noPdf') }
  }
  if (hintedMime === 'image/heic' || hintedMime === 'image/heif') {
    return { ok: false, mime: null, reason: i18n.global.t('image.noHeic') }
  }
  if (hintedMime === 'image/bmp' || hintedMime === 'image/tiff') {
    return { ok: false, mime: null, reason: i18n.global.t('image.unsupportedFormat', { format: hintedMime.replace('image/', '') }) }
  }
  return { ok: false, mime: null, reason: i18n.global.t('image.unsupportedType') }
}

/** 判断 MIME 是否在白名单 */
function isAllowedMime(mime: string): mime is AllowedMime {
  return (ALLOWED_IMAGE_MIMES as readonly string[]).includes(mime)
}

/**
 * 通过文件头 magic bytes 检测真实格式。
 * - PNG : 89 50 4E 47 0D 0A 1A 0A
 * - JPEG: FF D8 FF
 * - GIF : 47 49 46 38 (GIF87a / GIF89a 通用前缀)
 * - WEBP: 52 49 46 46 ?? ?? ?? ?? 57 45 42 50 (RIFF...WEBP)
 */
export function detectByMagic(bytes: Uint8Array): AllowedMime | null {
  if (bytes.length < 4) return null

  // PNG
  if (
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47
  ) {
    return 'image/png'
  }

  // JPEG
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return 'image/jpeg'
  }

  // GIF (GIF87a / GIF89a)
  if (
    bytes[0] === 0x47 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x38
  ) {
    return 'image/gif'
  }

  // WEBP: 0..3 = "RIFF", 8..11 = "WEBP"
  if (
    bytes.length >= 12 &&
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x46 &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x45 &&
    bytes[10] === 0x42 &&
    bytes[11] === 0x50
  ) {
    return 'image/webp'
  }

  return null
}

/**
 * 从 File 异步读取前 N 字节用于 magic 探测;
 * 已有 ArrayBuffer 时不必调用此函数,直接传给 validateImage。
 */
export async function readMagicProbe(file: Blob): Promise<Uint8Array> {
  const head = file.slice(0, MAGIC_PROBE_BYTES)
  const buf = await head.arrayBuffer()
  return new Uint8Array(buf)
}
