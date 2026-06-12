import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

/**
 * CLI 全局默认值(~/.claude/settings.json):顶栏模型/努力下拉的「默认」项
 * 展示真值用——会话未单独设置时,CLI 实际会用这些值。
 *
 * settings.json 是活文件(CLI 内 /effort 等实时改写),不能缓存死值
 * (pitfalls/cli-settings-live-rewrite):顶栏挂载与每次打开下拉时都重新拉取,
 * 模块级 ref 只是各实例间的共享展示位,不承担"权威缓存"职责。
 */

export interface CliSettings {
  /** settings.json 的 model 字段(可能是别名如 "sonnet";无字段为 null) */
  model: string | null
  /** settings.json 的 effortLevel 字段(无字段为 null,CLI 按模型自定默认) */
  effort_level: string | null
  /** settings.json 的 ultracode 全局开关(true 时默认行为即 Ultracode) */
  ultracode: boolean
}

const cliDefaults = ref<CliSettings>({ model: null, effort_level: null, ultracode: false })

export async function refreshCliDefaults(): Promise<void> {
  try {
    cliDefaults.value = await invoke<CliSettings>('get_cli_settings')
  } catch (_) {
    // 读取失败保留旧值:只影响「默认」标签的补充展示,不阻塞任何流程
  }
}

export function useCliDefaults() {
  return { cliDefaults, refreshCliDefaults }
}
