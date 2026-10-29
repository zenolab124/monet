import { computed, reactive, toValue, watch, type MaybeRefOrGetter } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { listModels } from '@/engines/client'
import { instanceKey } from '@/engines/identity'
import { LOCAL_CLAUDE_INSTANCE, resolveNativeInstance } from '@/engines/integration'
import type { EngineInstanceId } from '@/engines/types'
import {
  knownClaudeModelIds,
  pinnedClaudeModels,
  roleDisplayNames,
  type ReportedClaudeModel,
} from '@/utils/claudeModelCatalog'

/**
 * Claude Code 模型目录(按引擎实例缓存)。
 *
 * 每个实例两路来源:首次使用时读 Rust 侧该实例缓存的最近一次握手模型列表(engine_list_models;
 * 从未握手时它回退四个别名,过滤后等同"无数据");该实例会话握手时的 session-catalog
 * 事件推送最新列表覆盖。无数据时各派生值回退内置清单。缺省实例为本机实例。
 */
const reported = reactive(new Map<string, ReportedClaudeModel[]>())
const loading = new Map<string, Promise<void>>()
let listening = false

interface SessionCatalogPayload {
  engine?: EngineInstanceId
  catalog?: { models?: Array<{ value: string; display_name: string }> }
}

function reportedFor(instance: EngineInstanceId): ReportedClaudeModel[] {
  return reported.get(instanceKey(instance)) ?? []
}

/** 会话握手报出模型列表时覆盖该实例;空列表不覆盖 */
export function setReportedClaudeModels(
  models: ReadonlyArray<{ value: string; display_name: string }>,
  instance: EngineInstanceId = LOCAL_CLAUDE_INSTANCE,
) {
  if (!models.length) return
  reported.set(instanceKey(instance), models.map(model => ({ value: model.value, displayName: model.display_name })))
}

function listenSessionCatalog() {
  if (listening) return
  listening = true
  void listen<SessionCatalogPayload>('session-catalog', (event) => {
    setReportedClaudeModels(event.payload.catalog?.models ?? [], resolveNativeInstance(event.payload.engine))
  }).catch(error => console.warn('[model-catalog] 监听会话模型目录失败', error))
}

export function loadClaudeModelCatalog(instance: EngineInstanceId = LOCAL_CLAUDE_INSTANCE): Promise<void> {
  listenSessionCatalog()
  const key = instanceKey(instance)
  const pending = loading.get(key)
  if (pending) return pending
  const request = listModels({ engineId: instance.engineId, instanceId: instance.instanceId })
    .then((models) => {
      // 会话握手可能已先一步推送更新的列表
      if (reported.get(key)?.length) return
      reported.set(key, models.map(model => ({ value: model.model, displayName: model.displayName })))
    })
    .catch((error) => {
      console.warn('[model-catalog] 读取 Claude 模型目录失败,使用内置清单', error)
    })
  loading.set(key, request)
  return request
}

/** 非组件上下文(命令解析)用:该实例当前可接受的 /model 值 */
export function currentKnownClaudeModelIds(instance: EngineInstanceId = LOCAL_CLAUDE_INSTANCE): Set<string> {
  return knownClaudeModelIds(reportedFor(instance))
}

export function useClaudeModelCatalog(instance?: MaybeRefOrGetter<EngineInstanceId | null | undefined>) {
  const target = computed(() => resolveNativeInstance(toValue(instance)))
  watch(target, value => { void loadClaudeModelCatalog(value) }, { immediate: true })
  const models = computed(() => reportedFor(target.value))
  return {
    pinnedModels: computed(() => pinnedClaudeModels(models.value)),
    roleNames: computed(() => roleDisplayNames(models.value)),
  }
}
