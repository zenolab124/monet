/** 取路径最后一段（兼容 / 与 \ 分隔符） */
export function fileName(path: string): string {
  return path.split(/[/\\]/).filter(Boolean).pop() || path
}

/** 取父路径（去掉最后一段；根级或单段返回空串），保留原始分隔符风格 */
export function parentPath(path: string): string {
  const m = path.match(/^(.*)[/\\][^/\\]+[/\\]?$/)
  return m ? m[1] : ''
}

/** 家目录前缀缩写为 ~（mac `/Users/<name>`、Windows `X:\Users\<name>`） */
export function abbreviateHome(path: string): string {
  return path
    .replace(/^\/Users\/[^/]+/, '~')
    .replace(/^[A-Za-z]:[/\\]Users[/\\][^/\\]+/, '~')
}

/**
 * cwd → Claude projects 目录名。与 CLI 同规则：非字母数字一律 → `-`
 * （仅替换 `/` 会漏掉 `.`/`_` 及 Windows 的 `:`/`\`，致元数据落到错误项目目录）
 */
export function cwdToProjectId(cwd: string): string {
  return cwd.replace(/[^a-zA-Z0-9]/g, '-')
}

/**
 * 路径等价比较：分隔符归一（\ → /）、尾分隔符忽略；
 * Windows 形态（盘符开头）不区分大小写（NTFS 语义，用户常输小写盘符/正斜杠）
 */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '') || '/'
  const na = norm(a)
  const nb = norm(b)
  if (na === nb) return true
  return /^[A-Za-z]:\//.test(na) && na.toLowerCase() === nb.toLowerCase()
}
