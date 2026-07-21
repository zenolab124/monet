完美！现在让我生成最终的搜索报告：

## 搜索结果总结

在 `/Users/dev/monet` 项目中，我已找到所有相关的中文文本和状态判断逻辑。以下是详细的查找结果：

### 1. **中文文本出现位置**

#### 文件：`/Users/dev/monet/src/components/SessionDetail.vue`

| 行号 | 文本 | 上下文 |
|------|------|-------|
| 1072 | `会话正在后台运行,实时跟随中(进程结束后可继续输入)` | 输入框上方的状态提示信息 |
| 1109 | `会话在后台运行中…` | textarea placeholder（输入框禁用时） |
| 755 | `本地流式由 stream-event 实时驱动,无需外部探测` | 代码注释 |
| 808 | `后台结束的流式(本实例未挂载期间)留下脏标记` | 代码注释 |

#### 文件：`/Users/dev/monet/README.md`

| 行号 | 文本 |
|------|------|
| 13 | `文件监控：后台监听 JSONL 变化，自动刷新列表` |
| 10 | `流式响应：输入框发送跟进消息、实时流式渲染、Esc 中断` |

#### 文件：`/Users/dev/monet/src-tauri/src/channels.rs`

| 行号 | 文本 |
|------|------|
| 7 | `进程结束即删,应用启动兜底清空` |
| 355 | `进程结束后清理用` |
| 451 | `进程结束后删除本 turn 的 runtime 文件(含 token 副本,不留盘)` |

---

### 2. **状态判断逻辑实现**

#### **核心状态变量：`externalRunning`**

**文件**：`/Users/dev/monet/src/components/SessionDetail.vue`

**定义**（第 728 行）：
```typescript
const externalRunning = ref(false)
```

**用途**：
- 第 1108 行：控制输入框是否禁用 `:disabled="externalRunning"`
- 第 1130 行：控制发送按钮是否禁用 `:disabled="(!inputText.trim() && !imageInput.images.value.length) || externalRunning"`
- 第 1109 行：动态改变输入框 placeholder
- 第 1070-1072 行：显示/隐藏后台运行状态提示

---

#### **后台运行检测函数**

**文件**：`/Users/dev/monet/src/components/SessionDetail.vue`

**`probeExternal()` 函数**（第 750-785 行）：
```typescript
async function probeExternal() {
  if (probing) return
  probing = true
  try {
    const cs = currentSession.value
    // 本地流式由 stream-event 实时驱动,无需外部探测
    if (!cs || stream.value.streaming) {
      externalRunning.value = false
      stopExternalFollow()
      return
    }
    let running = false
    try {
      running = await invoke<boolean>('check_session_running', { sessionId: cs.summary.id })
    } catch {
      // 探测失败视为未运行
    }
    if (effectiveSessionId.value !== cs.summary.id) return
    if (running) {
      if (!externalRunning.value) {
        externalRunning.value = true
        followStreaming.value = true
      }
      await silentReloadRecords()
    } else if (externalRunning.value) {
      // 进程刚退出:收尾 reload 拿最终落账,结束跟随
      externalRunning.value = false
      await silentReloadRecords()
      stopExternalFollow()
    } else {
      stopExternalFollow()
    }
  } finally {
    probing = false
  }
}
```

**关键逻辑**：
- 调用 Tauri 命令 `check_session_running` 检查会话是否仍有 CLI 进程运行
- 每 1500ms 探测一次（第 791 行）
- 进程运行中：设置 `externalRunning = true`，启用实时跟随
- 进程退出后：设置 `externalRunning = false`，做最后一次刷新

---

#### **Rust 端实现**

**文件**：`/Users/dev/monet/src-tauri/src/commands.rs`

**`check_session_running()` 命令**（第 215-226 行）：
```rust
pub fn check_session_running(session_id: String) -> bool {
    // session_id 为 UUID,误匹配概率可忽略;再限定 claude 关键字防偶然碰撞
    let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "command"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|l| l.contains(&session_id) && l.contains("claude"))
}
```

**原理**：
- 使用 `ps -axo command` 获取所有进程命令行
- 搜索包含 session-id 和 "claude" 关键字的进程
- 如果找到，返回 `true`（进程仍在运行）

---

#### **会话状态判断**

**文件**：`/Users/dev/monet/src/composables/useSessionStatus.ts`

**`useSessionStatus()` composable**（第 40-65 行）：
```typescript
export function useSessionStatus(sessionId: Ref<string | null> | ComputedRef<string | null>) {
  const stream = useSessionStream(sessionId)
  const perms = queueForSession(sessionId)
  const { persistentToasts } = useNotifications()

  return computed<SessionStatus>(() => {
    const sid = sessionId.value
    let key: SessionStatusKey = 'idle'
    if (perms.value.length > 0) {
      key = 'waiting_permission'
    } else if (
      stream.value.streamError ||
      (sid && persistentToasts.value.some(t => t.kind === 'error' && t.sessionId === sid))
    ) {
      key = 'error'
    } else if (stream.value.streaming) {
      const tool = stream.value.activeTool
      if (tool?.startsWith('Workflow')) key = 'workflow'
      else if (tool) key = 'waiting_tool'
      else key = 'streaming'
    }
    return { key, ...STATUS_META[key] }
  })
}
```

**流式状态定义**（`useStreaming.ts` 第 39-56 行）：
```typescript
export interface SessionStreamState {
  streaming: boolean  // 本次流式是否进行中
  streamingTurns: StreamingTurn[]
  pendingUserMessage: string | null
  streamError: string | null
  startedAt: number | null
  activeTool: string | null  // 进行中的工具名（等待工具/Workflow 状态判定）
  tail: TailLine[]
  lastSent: { cwd: string; message: string; opts: SendOptions } | null
  realContextWindow: number | null
  realUsedTokens: number | null
}
```

---

### 3. **关键监控机制**

| 概念 | 位置 | 说明 |
|------|------|------|
| **后台结束脏标记** | `useStreaming.ts` 第 385 行 | `finishedDirty` Set，标记流式结束但未挂载的会话 |
| **静默重载** | `SessionDetail.vue` 第 734-748 行 | 不动加载态/滚动，仅当记录增长时替换 |
| **外部跟随启动** | `SessionDetail.vue` 第 787-793 行 | 监听会话切换，启动定时探测 |
| **实时滚动跟随** | `SessionDetail.vue` 第 676-717 行 | 流式中自动滚到底部 |

---

### 4. **流程总结**

**应用重启后的后台运行跟随流程**：

1. 用户切换到某会话 → `startExternalFollow()` 启动
2. 每 1500ms 调用 `probeExternal()` 检查是否有 CLI 进程
3. `check_session_running()` via ps 命令查看命令行
4. 若进程运行：`externalRunning = true`
   - 输入框禁用（placeholder: "会话在后台运行中…"）
   - 发送按钮禁用
   - 显示提示信息："会话正在后台运行,实时跟随中(进程结束后可继续输入)"
   - 启用滚动跟随，周期静默重载 JSONL 记录
5. 若进程退出：`externalRunning = false`
   - 做一次最终重载
   - 输入框恢复可用
   - 停止定时探测

这保证了即使应用窗口关闭后，后台 CLI 子进程也能继续运行任务，重新打开应用后能无缝继续跟随进度。