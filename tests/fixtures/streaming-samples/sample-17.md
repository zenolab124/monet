Perfect! Now let me create a concise summary with the key files and findings:

## 总结

我已完成对 monet 项目工作台消息流式渲染重复 bug 的深度分析。以下是核心发现：

### 关键代码位置与调用关系

**前端核心链路**:
1. `/Users/dev/monet/src/components/SessionDetail.vue`
   - **L845-882**: `watch(stream.streaming)` - 流式结束后重新加载 JSONL 记录的触发点
   - **L455-470**: `pendingUserUuid` computed - **关键问题区**，判定新 JSONL user 消息是否应被过滤
   - **L472-497**: `messages` computed - 执行过滤逻辑，调用 `pendingUserUuid`
   - **L525-538**: `messageGroups` computed - 对过滤后的消息分组渲染

2. `/Users/dev/monet/src/composables/useStreaming.ts`
   - **L109-135**: `getStream()` - 管理每会话的流式状态（streamingTurns/pendingUserMessage）
   - **L800-809**: `clearStreamingTurns()` - 清空流式消息，但在某些时序下可能不及时
   - **L717-736**: `completeFinish()` - 流式结束，设置 streaming=false

3. `/Users/dev/monet/src-tauri/src/parser.rs`
   - **L27-60**: `parse_messages()` - Rust 侧 JSONL 解析，不去重

### 根本原因

**Bug #1：pendingUserUuid 的条件过严 (L456)**
```typescript
if (!stream.value.pendingUserMessage || !stream.value.streamingTurns.length) return null
                                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```
当 `streamingTurns` 被清空但 `pendingUserMessage` 仍存在时，该计算属性返回 `null`，导致无法识别新 JSONL 中的对应 user 消息 uuid，历史区无法过滤该消息，与流式区的 `pendingUserMessage` 重复显示。

**时序竞态条件**:
- 流式结束 → `streaming=false` → 触发 watch
- watch 内等待 300ms 再加载 JSONL
- 期间 `pendingUserMessage` 和 `streamingTurns` 仍在 UI 显示
- JSONL reload 完成后，`pendingUserUuid` 因依赖条件不满足而返回 `null`
- 导致历史区新 JSONL 的 user 记录无法被过滤掉

### 修复方案（推荐）

**方案 1**（最直接）：修改 L456 的条件判定
```typescript
// 删掉 streamingTurns.length 的检查，只依赖 pendingUserMessage
if (!stream.value.pendingUserMessage) return null
```

**方案 2**（增强鲁棒性）：在 records reload 后立即清 streamingTurns（L880 后）
```typescript
if (newRecords) records.value = newRecords
if (!stream.value.streaming) {
  clearStreamingTurns(sid)
}
```

### 文件清单

| 文件 | 行号 | 函数/变量 | 作用 |
|-----|------|---------|-----|
| SessionDetail.vue | 455-470 | `pendingUserUuid` | **关键：判定新 JSONL user 消息是否为流式消息** |
| SessionDetail.vue | 472-497 | `messages` | 过滤消息，调用 pendingUserUuid |
| SessionDetail.vue | 525-538 | `messageGroups` | 消息分组 |
| SessionDetail.vue | 845-882 | `watch(stream.streaming)` | 流式结束后 JSONL reload 的触发器 |
| useStreaming.ts | 109-135 | `getStream()` | 流式状态管理入口 |
| useStreaming.ts | 800-809 | `clearStreamingTurns()` | 清空流式消息 |
| parser.rs | 27-60 | `parse_messages()` | JSONL 解析（Rust）|
| session_record.rs | 141-167 | `SessionRecord::from_json_owned()` | JSONL 反序列化 |