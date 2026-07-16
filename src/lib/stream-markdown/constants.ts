/**
 * 大文本截断阈值:超过该长度的文本块默认折叠显示前 N 字符(BlockText 展开按钮补全)。
 * 该值同时是「完成态渲染缓存 key」的截断口径——预热(useStreamSegments persistText、
 * SessionDetail settle 前预热)与历史区渲染(BlockText displayText)必须用同一常数,
 * 否则缓存 key 不一致导致换树时 miss → 同步 shiki 渲染卡帧。
 */
export const TEXT_TRUNCATE_LEN = 8192

/** 与历史区 BlockText 初始渲染(未展开)逐字节一致的缓存 key */
export function persistKeyOf(text: string): string {
  return text.length > TEXT_TRUNCATE_LEN ? text.slice(0, TEXT_TRUNCATE_LEN) : text
}
