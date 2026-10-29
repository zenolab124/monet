import { computed, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { listModels } from '@/engines/client'
import {
  knownClaudeModelIds,
  pinnedClaudeModels,
  roleDisplayNames,
  type ReportedClaudeModel,
} from '@/utils/claudeModelCatalog'

/**
 * Claude Code 模型目录(全局单例)。
 *
 * 来源两路:启动时读 Rust 侧缓存的最近一次握手模型列表(engine_list_models;
 * 从未握手时它回退四个别名,过滤后等同"无数据");Claude 会话握手时的 session-catalog
 * 事件推送最新列表覆盖。无数据时各派生值回退内置清单。
 */
const reported = ref<ReportedClaudeModel[]>([])
let loading: Promise<void> | null = null

/** 会话握手报出模型列表时覆盖;空列表不覆盖 */
export function setReportedClaudeModels(models: ReadonlyArray<{ value: string; display_name: string }>) {
  if (!models.length) return
  reported.value = models.map(model => ({ value: model.value, displayName: model.display_name }))
}

export function loadClaudeModelCatalog(): Promise<void> {
  if (loading) return loading
  void listen<{ catalog?: { models?: Array<{ value: string; display_name: string }> } }>('session-catalog', (event) => {
    setReportedClaudeModels(event.payload.catalog?.models ?? [])
  }).catch(error => console.warn('[model-catalog] 监听会话模型目录失败', error))
  loading = listModels({ engineId: 'claude-code', instanceId: 'default' })
    .then((models) => {
      // 会话握手可能已先一步推送更新的列表
      if (reported.value.length) return
      reported.value = models.map(model => ({ value: model.model, displayName: model.displayName }))
    })
    .catch((error) => {
      console.warn('[model-catalog] 读取 Claude 模型目录失败,使用内置清单', error)
    })
  return loading
}

/** 非组件上下文(命令解析)用:当前可接受的 /model 值 */
export function currentKnownClaudeModelIds(): Set<string> {
  return knownClaudeModelIds(reported.value)
}

export function useClaudeModelCatalog() {
  void loadClaudeModelCatalog()
  return {
    pinnedModels: computed(() => pinnedClaudeModels(reported.value)),
    roleNames: computed(() => roleDisplayNames(reported.value)),
  }
}
