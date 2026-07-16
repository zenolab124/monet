完美！现在我有足够的信息来提供一份完整的总结。让我生成最终报告。

## 搜索结果总结

我已经完成了对 cc-space-tauri 项目 Agent 相关基础设施的详尽分析。以下是关键发现：

### 1. **src/ 中的 Agent 相关 Composables 和工具函数**

**会话自动命名（AI自动标题）**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/useSessionMeta.ts`
  - `ensureTitle(projectId, sessionId)` — 异步触发标题生成，不阻塞发送流程
  - `triggerTitleGeneration(sessionId, cwd)` — 用户发送消息后调用，自动生成标题
  - 支持缓存，避免重复生成

**权限决策辅助（权限提示生成）**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/usePermissionHints.ts`
  - `requestHint(requestId, toolName, input)` — 为权限审批请求生成中文解释
  - 本地缓存机制，按 `toolName + 关键参数签名` 去重

**工坊资产管理（Skills/Commands/Agents/MCP 四类资产）**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/useWorkshop.ts`
  - `useWorkshop()` — 一体化工坊数据源
  - `probeMcpServers()` — 对 http/sse 类型 MCP 服务器并发探活

**数据模型定义**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src/types/index.ts`
  - `WorkshopAgent` 接口（Subagent 定义）
  - `WorkshopAssets` 接口（四类资产统一容器）
  - `SessionRecord` 联合类型包含 `ai_title` 记录类型

---

### 2. **src-tauri/ 中的 Agent 相关 Rust 模块**

**元数据与 AI 调用层（核心）**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/metadata.rs` (249 行)
  
  **关键函数：**
  - `generate_title(project_id, session_id) -> Result<String>` 
    - 异步生成会话标题
    - 调用 Claude CLI 进程（纯 CLI 集成，无 API key 管理）
    - **LLM 调用方式**：`Command::new("claude")` + 专用 system prompt
    - 模型：`claude-haiku-4-5-20251001`
    - 参数：`--effort low --output-format text`
    - 工作目录：`~/.claude/cc-space-agent/`（隔离 agent JSONL）
    
  - `generate_permission_hint(tool_name, input_json) -> Result<String>`
    - 生成权限请求的中文批注（≤50字）
    - 同样走 CLI，使用 `--no-session-persistence` 避免误入用户会话

  - `update_meta(session_id, patch)` — 更新会话元数据
  - `get_all_meta()` — 加载所有元数据（~/.claude/cc-space/metadata.json）

**工坊资产扫描层**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/workshop.rs` (500+ 行)
  
  **关键函数：**
  - `get_workshop_assets() -> Result<WorkshopAssets>`
    - 全局 + 项目级标准路径全量扫描
    - 返回 Skills / Commands / Subagents / MCP 四类资产
  
  - `scan_agents_dir(dir, source) -> Vec<AgentAsset>`
    - 扫描 `~/.claude/agents/` 和 `<project>/.claude/agents/`
    - 单层扫描，每个 `*.md` 文件是一个 Subagent
    - 从 frontmatter 提取 name/description，缺失用文件名
  
  - `probe_mcp_server(url) -> Result<bool>` — 检测 MCP 服务器在线状态
  - `open_workshop_dir(category)` — 打开工坊目录

**模型定义**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/models/session_record.rs`
  - `AiTitleRecord` — 会话标题记录结构
  - `SessionRecord::AiTitle(AiTitleRecord)` — 枚举变体，type="ai-title"

**自动化域**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/automation.rs` (100+ 行)
  - Hooks 配置读取与运行统计（暂无 Agent 直接调用，架构预留）

**命令层暴露接口**
- `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/lib.rs`
  ```rust
  metadata::get_all_meta,
  metadata::update_meta,
  metadata::generate_title,
  metadata::generate_permission_hint,
  workshop::get_workshop_assets,
  workshop::probe_mcp_server,
  workshop::open_workshop_dir,
  ```

---

### 3. **统一的 LLM 调用抽象层（架构特点）**

**核心设计原则**（见 `/Users/zz/workspace/cc-apps/cc-space-tauri/docs/agent-iframe-architecture.md`）：

- **走 Claude CLI，不直接管理 API key**
  - `Command::new("claude")` spawn 一次性进程
  - 复用 CLI 现有认证体系（~/.claude/claude.json）
  - 用完即退，无连接池开销

- **LLM Provider 固定为 Anthropic Claude**
  - 模型：`claude-haiku-4-5-20251001`（轻量级，成本最低）
  - 无 provider 切换机制

- **Prompt 管理**（嵌入 metadata.rs）
  ```rust
  // 标题生成 system prompt
  "你是一个标题生成器。根据对话内容生成一个10字以内的中文标题。只输出标题本身，不要加引号、标点或任何其他内容。"
  
  // 权限提示 system prompt
  "你是权限决策助手。用户正在审批一个工具调用请求。用一句简洁的中文解释这个操作在做什么，如有风险请指出。只输出解释本身，不超过50字。"
  ```

- **会话隔离（防止 Agent 调用混入用户数据）**
  - Agent JSONL 路径：`~/.claude/projects/-Users-<user>-.claude-cc-space-agent/`
  - discovery 模块跳过该路径（路径前缀过滤）
  - Agent 调用加 `--no-session-persistence` 时不落盘

---

### 4. **已实现的 Agent 功能**

| 场景 | 实现状态 | 入口 | LLM 调用 |
|------|--------|-----|---------|
| **会话自动命名** | ✅ 生产就绪 | `useSessionMeta.ensureTitle()` | `generate_title` command |
| **权限决策辅助** | ✅ 生产就绪 | `usePermissionHints.requestHint()` | `generate_permission_hint` command |
| **Subagent 资产发现** | ✅ 生产就绪 | `useWorkshop.assets.agents` | `get_workshop_assets` command |
| **MCP 在线探测** | ✅ 生产就绪 | `useWorkshop.probeMcpServers()` | `probe_mcp_server` command |
| **iframe 沙箱通信** | ✅ 架构验证 | `AgentIframeDemo.vue` | 双向 postMessage |
| 设置字段翻译 | 📋 规划中 | — | — |
| 未知卡片适配 | 📋 规划中 | — | — |
| 上下文诊断建议 | 📋 规划中 | — | — |

---

### 5. **LLM Provider 配置**

**无显式配置文件**（所有配置硬编码）：
- 模型：`claude-haiku-4-5-20251001`（Rust metadata.rs 中硬编码）
- API 认证：复用 `~/.claude/claude.json`（Claude CLI 管理）
- 工作目录隔离：`~/.claude/cc-space-agent/`
- 元数据存储：`~/.claude/cc-space/metadata.json`（JSON 扁平化存储）

**无 API key 管理层**（关键设计点）：
- 不存在 `.env` / `secrets.json`
- 不涉及 OpenAI / Gemini / 其他 provider
- 100% 依赖 CLI 的渠道认证机制

---

### 6. **关键文件路径总结**

| 文件 | 用途 | 重点接口 |
|-----|-----|--------|
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/useSessionMeta.ts` | 标题生成控制 | `ensureTitle()`, `triggerTitleGeneration()` |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/usePermissionHints.ts` | 权限提示缓存 | `requestHint()`, `getHint()` |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src/composables/useWorkshop.ts` | 工坊资产管理 | `useWorkshop()`, `probeMcpServers()` |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/metadata.rs` | AI 调用核心 | `generate_title()`, `generate_permission_hint()` |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/workshop.rs` | 资产扫描 | `get_workshop_assets()`, `scan_agents_dir()` |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/src-tauri/src/models/session_record.rs` | 数据模型 | `AiTitleRecord`, `SessionRecord::AiTitle` |
| `/Users/zz/workspace/cc-apps/cc-space-tauri/docs/agent-iframe-architecture.md` | 架构设计文档 | 通信协议、隔离策略、场景规划 |

---

### 7. **架构设计特点**

1. **CLI 优先**：所有 LLM 调用走 `claude` CLI，复用认证体系
2. **轻量级**：使用 Haiku（成本 + 速度优化），`--effort low`
3. **隔离首则**：Agent JSONL 物理分离，不污染用户会话
4. **缓存机制**：权限提示按签名缓存，避免重复 API 调用
5. **诚实降级**：标注"AI 生成"，失败 fallback 原始数据
6. **iframe 沙箱**：预留未来复杂 UI 渲染（目前仅文本）

这个项目的 Agent 基础设施采用了**最小化设计**——避免复杂的 API 代理或 provider 切换，而是直接依赖 CLI 的既有认证机制，让 agent 功能自然集成到 CC Space 的数据流中。