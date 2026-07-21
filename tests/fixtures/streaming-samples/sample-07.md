完美。现在我有所有需要的信息。让我生成最终的调研结论。

## 调研结论：CC Space 工坊域 PRD 基础数据

### 1. 四类资产存储位置与格式

#### A. **Skills**（技能）

**全局存储**：`~/.claude/skills/`

- 方式：符号链接 + 本地目录混合
- **SKILL.md Frontmatter 字段**（样例来自 `ui-styling/SKILL.md`）：
  ```yaml
  ---
  name: ckm:ui-styling
  description: [长描述文本]
  argument-hint: "[component or layout]"
  license: MIT
  metadata:
    author: claudekit
    version: "1.0.0"
  ---
  ```
  - 包含字段：`name`, `description`, `argument-hint` (可选), `license` (可选), `metadata.version` (可选, 部分有)
  - **有 version 字段**，位于 metadata 下或缺失

**插件内 Skills**：`~/.claude/plugins/marketplaces/*/skills/SKILL.md`（共18个SKILL.md）

**全局 Skills 清单**（10个）：
```
blog, cartographer, codewise, fix-thinking, prd, product-optimize, 
recall, ui-styling, ui-ux-pro-max, unocss
```

#### B. **Commands**（自定义命令）

**不存在全局顶级 `~/.claude/commands/` 目录**

**插件内存储**：`~/.claude/plugins/[marketplace]/[plugin]/commands/*.md`（共36个）

**Command Frontmatter**（样例来自 `ralph-loop.md`）：
```yaml
---
description: "描述文本"
argument-hint: "PROMPT [--max-iterations N]"
allowed-tools: ["Bash(...)", "..."]
hide-from-slash-command-tool: "true"
---
```
- 字段：`description`, `argument-hint` (可选), `allowed-tools` (可选), `hide-from-slash-command-tool` (可选)
- **无 version 字段，无 name 字段**（命令名由文件名推导）

#### C. **Agents**（后台代理）

**不存在全局顶级 `~/.claude/agents/` 目录**

**插件内存储**：`~/.claude/plugins/[marketplace]/[plugin]/agents/*.md`（共27个）

**Agent Frontmatter**（样例来自 `plugin-validator.md`）：
```yaml
---
name: plugin-validator
description: |
  [多行描述，含示例与触发条件]
model: inherit
color: yellow
tools: ["Read", "Grep", "Glob", "Bash"]
---
```
- 字段：`name`, `description`, `model` (可选), `color` (可选), `tools` (可选)
- **无 version 字段**

#### D. **MCP 服务器**

**全局配置**：`.mcp.json` 文件分布在插件目录中（共16个 `.mcp.json` 配置文件）

**存储结构样例**（来自 `discord/.mcp.json`）：
```json
{
  "mcpServers": {
    "discord": {
      "command": "bun",
      "args": ["run", "--cwd", "${CLAUDE_PLUGIN_ROOT}", "--shell=bun", "--silent", "start"]
    }
  }
}
```

**另一样例**（来自 `gitlab/.mcp.json`，HTTP 类型）：
```json
{
  "gitlab": {
    "type": "http",
    "url": "https://gitlab.com/api/v4/mcp"
  }
}
```

**MCP 服务器清单**（20个）：
```
discord, terraform, gitlab, context7, linear, greptile, serena, playwright, 
fakechat, firebase, github, telegram, laravel-boost, imessage, asana, 
example-server, 以及其他 stdio 类型配置
```

**settings.json 中无 mcpServers 字段** — MCP 配置完全由 `.mcp.json` 管理或 plugin.json 中的 mcpServers 字段

---

### 2. 启停机制真相

**Claude Code 存在 MCP 的 disable 机制，但不存在 Skill/Command/Agent 的全局 disable**

#### MCP 启停机制：
- **命令**：`/mcp enable [server-name]` 或 `/mcp disable [server-name]`
- **UI**：VSCode 扩展的 `/mcp` 对话框可直接 toggle 启停服务器
- **配置**：settings.json 中 **无 disable/enabled 字段**，通过运行时 toggle 控制

#### Skill 机制：
- 全局有 `disableBundledSkills` 设置和 `CLAUDE_CODE_DISABLE_BUNDLED_SKILLS` 环境变量（仅禁用**捆绑 skills**，不针对单个）
- 单个 Skill **无 toggle 机制**，也没有 enable/disable 字段

#### Command/Agent 机制：
- **无 disable 机制**，无任何配置字段支持
- 插件级别有 `/plugin enable/disable`，但不针对单个 command/agent

**PRD 设计建议**：
- MCP：保留 toggle（已有官方支持）
- Skills/Commands/Agents：toggle 需要自定义实现（可映射到：插件禁用、frontmatter 新增 `enabled: false` 字段、或项目级配置）

---

### 3. MCP 探活可行性

#### **stdio 类型成本/风险**：
- 成本：每次探活需 `spawn` 进程 → 建立 MCP initialize 握手 → 等待响应 → 关闭进程
- 风险：进程启动可能失败（command 不存在、权限问题、$VAR 引用未定义）、超时（无 timeout 配置时用 MCP_TOOL_TIMEOUT 或默认）、资源泄漏

#### **HTTP 类型**：
- 成本低：简单 HTTP GET/OPTIONS 探测
- 风险低：网络不稳定、服务宕机返回非 200

#### **Claude Code 缓存**：
- `~/.claude/mcp-needs-auth-cache.json`：仅记录需要 OAuth 认证的服务器
  ```json
  {
    "claude.ai Google Drive": {
      "timestamp": 1781227618897,
      "id": "mcpsrv_01KyR6pAe6jzAXeVD35xBJyT"
    }
  }
  ```
  **无通用状态缓存**（在线/离线状态）

#### **探活可行性结论**：
- **HTTP 类型**：直接 ping 可行
- **stdio 类型**：可行但成本高，建议：
  1. 缓存探活结果（TTL = 5-10 分钟）
  2. 异步后台探活（不阻塞工坊 UI）
  3. 提供超时配置（默认 2-3 秒）

---

### 4. 项目级资产

**项目** `/Users/dev/monet/.claude/`：
- 仅包含 `.DS_Store`（无功能性文件）
- **无项目级 SKILL.md、command、agent、.mcp.json**

**项目 CLAUDE.md** 中：
- 知识库入口：`docs/knowledge/INDEX.md`（由 codewise skill 维护）
- 无资产清单声明

**合并覆盖关系**：
- 全局 → 项目级：全局资产始终可见，项目级资产补充/覆盖（如项目有同名 skill）
- 现状：项目无额外资产，仅展示全局资产

---

### 5. 本机数量实测

#### **全局资产总数**：

| 类型 | 数量 | 清单/说明 |
|-----|------|---------|
| **Global Skills** | 10 | blog, cartographer, codewise, fix-thinking, prd, product-optimize, recall, ui-styling, ui-ux-pro-max, unocss |
| **Plugins 内 Commands** | 36 | 分布在 hookify, ralph-loop, planning-with-files, cq 等插件 |
| **Plugins 内 Agents** | 27 | 分布在 plugin-dev, feature-dev, hookify, code-simplifier 等插件 |
| **Marketplace Skills** | 18 | 在 marketplaces/anthropic-agent-skills/skills 下（theme-factory, doc-coauthoring, claude-api, xlsx 等） |
| **MCP 服务器配置** | 16 × `.mcp.json` | discord, terraform, gitlab, github, firebase, linear, asana 等（共20个唯一服务器名）  |
| **MCP 需认证缓存** | 1 | claude.ai Google Drive（仅显示需 OAuth 的服务器） |

#### **存储位置层级**：
```
~/.claude/
├── skills/                              # 10 个全局 skills（含符号链接）
└── plugins/
    ├── marketplaces/
    │   ├── anthropic-agent-skills/      # 18 个官方 skills
    │   ├── claude-plugins-official/
    │   │   ├── plugins/*/commands/      # 多个 commands
    │   │   ├── plugins/*/agents/        # 27 个 agents
    │   │   ├── external_plugins/*/      # MCP 配置
    │   └── cq/                          # 其他插件
    ├── cache/                           # 缓存的插件
    └── ...
├── mcp-needs-auth-cache.json           # MCP 认证状态快照
└── settings.json                        # 无 mcpServers 字段
```

---

### 关键结论供 PRD 参考

1. **四类资产的 UI 展示**：
   - Skills：全局 10 + 插件内 18 = 28 个，每个有版本（metadata.version）
   - Commands：仅插件级，共 36 个，无版本（可考虑 frontmatter 扩展）
   - Agents：仅插件级，共 27 个，无版本
   - MCP 服务器：16 个配置文件，20 个唯一服务器

2. **启停 toggle 范围**：
   - **必要**：MCP 服务器（已有官方 `/mcp enable/disable`）
   - **可选/待设计**：Skills（仅 bundled 有全局禁用）、Commands、Agents（无机制）
   - **建议方案**：在 Skill/Command/Agent frontmatter 新增 `enabled: true|false` 字段 + settings.json 记录禁用清单

3. **MCP 探活成本**：
   - stdio 类型：需 spawn 进程（成本 100-500ms），建议异步后台 + 缓存
   - HTTP 类型：简单 HTTP 探测（成本 10-50ms）
   - 无官方状态缓存，建议工坊内维护 TTL 缓存

4. **项目级覆盖**：
   - 当前项目无自定义资产，全部来自全局
   - 支持项目级 `.claude/skills/` 等覆盖全局同名资产