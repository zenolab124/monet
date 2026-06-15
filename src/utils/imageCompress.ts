import i18n from '../locales'

/**
 * 图片压缩工具:
 * - >5MB → 用 Canvas 重绘为 JPEG quality=80,目标 ≤1MB
 * - 压缩后仍 >2MB → 拒绝(返回 null + 原因)
 * - ≤5MB → 不压缩,原样返回
 *
 * PRD FR-005:单张大小限制
 */

/** 单张限制 */
export const SIZE_THRESHOLD_COMPRESS = 5 * 1024 * 1024 // 5MB
export const SIZE_THRESHOLD_REJECT_AFTER = 2 * 1024 * 1024 // 2MB
export const COMPRESS_TARGET = 1 * 1024 * 1024 // 1MB

/** 压缩输出 */
export interface CompressResult {
  /** 是否成功(>2MB after compress 视为失败) */
  ok: boolean
  /** 压缩或原始的 Blob;失败时为 null */
  blob: Blob | null
  /** 最终 MIME(压缩后会变成 image/jpeg;不压缩时保留原 MIME) */
  mime: string
  /** 失败原因 */
  reason: string | null
}

/**
 * 处理单张图片:按需压缩。
 *
 * @param input    原始 Blob(File 或 fetch 出来的 Blob 均可)
 * @param mime     已校验通过的 MIME(用于决定是否走 Canvas);PRD 允许的 4 种之一
 */
export async function compressIfNeeded(
  input: Blob,
  mime: string,
): Promise<CompressResult> {
  // 小于阈值不压缩
  if (input.size <= SIZE_THRESHOLD_COMPRESS) {
    return { ok: true, blob: input, mime, reason: null }
  }

  // 走 Canvas 压缩
  try {
    const compressed = await canvasCompressToJpeg(input, 0.8)
    if (compressed.size > SIZE_THRESHOLD_REJECT_AFTER) {
      return {
        ok: false,
        blob: null,
        mime: 'image/jpeg',
        reason: i18n.global.t('image.tooLarge'),
      }
    }
    return { ok: true, blob: compressed, mime: 'image/jpeg', reason: null }
  } catch (e) {
    return {
      ok: false,
      blob: null,
      mime,
      reason: i18n.global.t('image.compressFailed', { error: (e as Error).message || String(e) }),
    }
  }
}

/**
 * Canvas 压缩为 JPEG。
 * - 优先用 createImageBitmap(支持 png/jpeg/gif/webp,并能正确处理 EXIF)
 * - 不缩放尺寸;只通过 toBlob(quality) 控制大小。如果像素过大可在外层再加缩放
 */
async function canvasCompressToJpeg(blob: Blob, quality: number): Promise<Blob> {
  const bitmap = await createImageBitmap(blob)
  const canvas = document.createElement('canvas')
  canvas.width = bitmap.width
  canvas.height = bitmap.height
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('Canvas 2D 上下文不可用')

  // JPEG 不支持透明,先填白底,避免 alpha 通道转黑
  ctx.fillStyle = '#FFFFFF'
  ctx.fillRect(0, 0, canvas.width, canvas.height)
  ctx.drawImage(bitmap, 0, 0)
  bitmap.close()

  return await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(
      (b) => {
        if (b) resolve(b)
        else reject(new Error('canvas.toBlob 返回 null'))
      },
      'image/jpeg',
      quality,
    )
  })
}

/**
 * Blob → base64 字符串(无 data URL 前缀,纯 base64)
 *
 * 用于:
 * - 发送给 claude CLI 时构造 image block 的 source.data
 */
export async function blobToBase64(blob: Blob): Promise<string> {
  return await new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = reader.result as string
      // result 形如 "data:image/png;base64,iVBORw0KGgo..."
      const idx = result.indexOf(',')
      resolve(idx >= 0 ? result.slice(idx + 1) : result)
    }
    reader.onerror = () => reject(reader.error ?? new Error('FileReader 失败'))
    reader.readAsDataURL(blob)
  })
}

/**
 * Blob → data URL(用于 <img :src> 显示缩略图)
 */
export async function blobToDataUrl(blob: Blob): Promise<string> {
  return await new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = () => reject(reader.error ?? new Error('FileReader 失败'))
    reader.readAsDataURL(blob)
  })
}
