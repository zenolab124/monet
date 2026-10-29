import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

function source(path: string): string {
  return readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8')
}

describe('native controller instance pass-through', () => {
  it('threads the session instance into the native session detail', () => {
    const unified = source('../../src/components/session/UnifiedSessionDetail.vue')
    const detail = source('../../src/components/SessionDetail.vue')
    const topBar = source('../../src/components/topbar/SessionTopBar.vue')
    const capsule = source('../../src/components/topbar/RunConfigCapsule.vue')

    expect(unified).toContain(':engine="session?.engine ?? null"')
    expect(detail).not.toContain("instanceId: 'default'")
    expect(detail).toContain('const claudeEngineInstance = computed(() => resolveNativeInstance(')
    expect(detail).toContain('useModelOptions(resolvedChannelId, claudeEngineInstance)')
    expect(detail).toContain('instance: claudeEngineInstance.value')
    expect(detail).toContain(':native-instance="claudeEngineInstance"')
    expect(topBar).toContain(':native-instance="nativeInstance"')
    expect(capsule).toContain('useModelOptions(channelRef, () => props.nativeInstance)')
  })

  it('records the instance on every native draft entry point', () => {
    const picker = source('../../src/components/workbench/NewTaskEnginePicker.vue')
    const column = source('../../src/components/workbench/WorkbenchColumn.vue')
    const columns = source('../../src/components/workbench/WorkbenchColumns.vue')
    const race = source('../../src/composables/useRaceInput.ts')
    const workbench = source('../../src/composables/useWorkbench.ts')

    expect(picker).toContain('promotePendingTaskToDraft(props.sessionId, instance)')
    expect(picker).toContain('stageDraftSession(props.cwd, nativeSessionId, instance)')
    expect(column).toContain('registerFork(newSessionId, props.column.sessionId, session.cwd, nativeInstance.value)')
    expect(column).toContain('createDraftSession(cwd, nativeInstance.value)')
    expect(column).toContain(':current-instance-key="currentInstanceKey"')
    expect(columns).toContain('registerFork(newSessionId, sessionId, cwd, session?.engine ?? nativeDraftInstance(sessionId))')
    expect(race).toContain('stageDraftSession(race.cwd, undefined, target.instance)')
    expect(race).toContain('registerFork(newSessionId, sourceLane.sessionId, race.cwd, context.nativeInstance)')
    expect(race).toContain('stageDraftSession(race.cwd, undefined, context.nativeInstance)')
    expect(race).toContain('choices.find(engine => sameInstance(engine.instance, context.nativeInstance))')
    // 本机草稿不落实例字段,旧持久化数据形状不变
    expect(workbench).toContain('if (instance && !sameInstance(instance, LOCAL_CLAUDE_INSTANCE))')
    expect(workbench).toContain('if (!state.value.drafts[sid]) delete state.value.nativeDraftInstances[sid]')
  })

  it('matches incremental project changes by the instance that produced them', () => {
    const projects = source('../../src/composables/useProjects.ts')
    const watcher = source('../../src-tauri/src/watcher.rs')
    const streaming = source('../../src-tauri/src/streaming.rs')

    expect(watcher).toContain('payload["instance"] = json!(instance)')
    expect(projects).toContain('applySessionChanges(payload.changes, resolveNativeInstance(payload.instance))')
    expect(projects).toContain('sameInstance(p.engine, instance) && usesNativeSessionSurface(p.engine)')
    expect(streaming).toContain('"engine": instance')
  })
})
