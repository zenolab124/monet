export function fileName(path: string): string {
  return path.split('/').pop() || path
}
