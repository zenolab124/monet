两个探索 Agent 已在后台并行工作：一个摸工作台的布局和能力，一个摸 Paper 主题的样式细节。等结果回来后我给你一个完整的构思方案。

<details>
<summary>agent:a3ea28f7a55d846c8 completed</summary>

Here's a comprehensive overview of the CC Space Workbench UI layout and capabilities:

## Overall Layout Structure

The main app (`App.vue`) uses a multi-domain view architecture with an **Activity Bar** (48px wide) on the left that switches between 6 view domains using `v-show` to keep DOM resident:

1. **Home** - Landing/dashboard view
2. **Workbench** (工作台) - Active session workspace
3. **Archive** (档案馆) - Read-only session browsing
4. **Workshop** (工坊) - Agent workshop
5. **Automation** (自动化) - Automation view
6. **Settings** (设置) - App settings

## Workbench Layout (`src/views/workbench/WorkbenchView.vue`)

The workbench has a **3-zone layout**:

### Zone 1: MonitorRail (Left sidebar, ~220px)
Located at `src/views/workbench/MonitorRail.vue`:
- Shows a list of **active sessions** being monitored
- Each session card (`MonitorCard.vue`) displays:
  - Session title/project name
  - Active status indicator (pulsing dot for streaming)
  - Token usage (input/output/cache)
  - Cost display
  - Model name
  - Last message preview
  - Timestamp
  - Permission request alerts (tool approvals needed)
- Supports **drag-and-drop** reordering via `@dnd-kit/vue`
- Click a card to open/focus that session
- Cards show real-time streaming status

### Zone 2: Tab Bar (Top)
Located at `src/views/workbench/WorkbenchTabs.vue`:
- Tab-based multi-context management
- Each tab represents a **column layout** (not individual sessions)
- Tabs can be:
  - Created (+ button)
  - Closed (× button) 
  - Renamed (double-click)
  - Reordered via drag-and-drop
- Shows active tab indicator
- **Run Config Capsule** (`RunConfigCapsule.vue`) in the tab bar area:
  - Shows current model/channel selection
  - Click to expand configuration panel
  - Displays model name, effort level, channel info

### Zone 3: WorkbenchColumns (Main content area)
Located at `src/views/workbench/WorkbenchColumns.vue`:
- **Multi-column layout** supporting side-by-side session viewing
- Each column contains a `SessionPanel` showing one session
- Columns can be:
  - Added (split view)
  - Removed
  - Resized (drag column dividers)
- Supports drag-and-drop of sessions between columns

## Session Panel (`src/views/workbench/SessionPanel.vue`)

Each session panel renders a full conversation with:

### Message Types:
- **Human messages**: User input with avatar
- **Assistant messages**: AI responses with:
  - **Thinking blocks** (`ThinkingBlock.vue`): Collapsible reasoning display with token count badges
  - **Text content**: Markdown rendered with syntax highlighting (Shiki)
  - **Tool calls** (`ToolCallBlock.vue`): Collapsible tool invocations showing:
    - Tool name with icon
    - Input parameters
    - Output/results
    - Duration
    - Status (success/error/pending)
  - **Code blocks**: Syntax-highlighted with copy button
  - **Diff display**: File change diffs with syntax highlighting
- **System messages**: Status indicators, errors

### Tool Call Types Displayed:
From the tool icon mapping (`toolIcons` in ToolCallBlock), supported tools include:
- Read, Write, Edit (file operations)
- Bash (terminal commands)  
- Search/Grep
- WebFetch, WebSearch
- Agent (sub-agent spawning)
- TaskCreate, TaskUpdate
- Notebook operations
- MCP tool calls
- And many more

### Sub-Agent Display:
- Sub-agents appear as expandable nested sections within the conversation
- Each sub-agent shows its own message thread
- Located via `SubagentBlock.vue` - renders agent conversations inline
- Shows agent label, status, and full conversation thread

## Streaming Interaction
- Real-time token-by-token rendering
- Streaming status shown in MonitorCard (pulsing indicator)
- Live markdown rendering during stream
- Tool calls appear progressively as they execute
- Permission requests surface in MonitorRail cards

## Search Functionality
- **Global search** (`SearchView.vue`): Full-text search across all sessions
- **Semantic search mode**: AI-powered search using Rust engine
- Search results show matched sessions with context snippets
- Click result to navigate to session

## Channel/Model Selection (`RunConfigCapsule.vue` + `RunConfigPanel.vue`)
- Capsule in tab bar shows current configuration
- Expandable panel for:
  - Channel selection (native Claude, OpenAI-compatible endpoints)
  - Model selection
  - Effort level (low/medium/high/max)
  - Channel presets

## Drag-and-Drop System
Uses `@dnd-kit/vue` throughout:
- MonitorRail: Reorder session cards
- Tab bar: Reorder tabs
- Columns: Move sessions between columns
- Archive: Organize sessions

## Key Component Files:
- `src/views/workbench/WorkbenchView.vue` - Main workbench container
- `src/views/workbench/MonitorRail.vue` - Left session list
- `src/views/workbench/MonitorCard.vue` - Individual session card
- `src/views/workbench/WorkbenchTabs.vue` - Tab management
- `src/views/workbench/WorkbenchColumns.vue` - Multi-column layout
- `src/views/workbench/SessionPanel.vue` - Session conversation view
- `src/components/session/MessageBubble.vue` - Message rendering
- `src/components/session/ToolCallBlock.vue` - Tool call display
- `src/components/session/ThinkingBlock.vue` - Thinking block display
- `src/components/session/SubagentBlock.vue` - Sub-agent display
- `src/views/workbench/RunConfigCapsule.vue` - Model/channel selector
- `src/App.vue` - Root with Activity Bar

</details>

<details>
<summary>agent:a7f16096081b7d2d4 completed</summary>

Here are the key Paper theme styling details for CC Space:

## Paper Theme CSS Variables (from `src/assets/styles/paper-tokens.css`)

### Light Mode Colors
```
--color-bg: #F5F0E8          (warm cream/parchment background)
--color-bg-secondary: #EDE7DB (slightly darker cream)
--color-bg-tertiary: #E5DED1  (even darker, for deeper layers)
--color-surface: #FAF7F2      (card/panel surface)
--color-surface-hover: #F0EBE1
--color-surface-active: #E8E2D6
--color-border: #D6CBBA       (warm brown border)
--color-border-subtle: #E0D8CA
--color-text: #2C2C2C         (near-black ink)
--color-text-secondary: #6B6560
--color-text-tertiary: #9B9590
--color-accent: #2D5F2D       (ink green - primary accent)
--color-accent-hover: #1E4A1E
--color-accent-secondary: #8B4513 (brick red/saddle brown)
--color-shadow-1: rgba(120, 100, 70, 0.08)  (brown-tinted shadows, 3 levels)
--color-shadow-2: rgba(120, 100, 70, 0.12)
--color-shadow-3: rgba(120, 100, 70, 0.18)
```

### Dark Mode Colors (`.dark` class)
```
--color-bg: #1A1A1A
--color-bg-secondary: #242424
--color-bg-tertiary: #2E2E2E
--color-surface: #1E1E1E
--color-surface-hover: #2A2A2A
--color-surface-active: #333333
--color-border: #3A3A3A
--color-border-subtle: #2E2E2E
--color-text: #E8E0D4         (warm off-white)
--color-text-secondary: #A09A90
--color-text-tertiary: #706A60
--color-accent: #5A9A5A       (lighter green for dark bg)
--color-accent-hover: #6BAF6B
--color-accent-secondary: #C17B4A
--color-shadow-1: rgba(0, 0, 0, 0.2)
--color-shadow-2: rgba(0, 0, 0, 0.3)
--color-shadow-3: rgba(0, 0, 0, 0.4)
```

### Layout Tokens
```
--radius-sm: 4px
--radius-md: 6px
--radius-lg: 8px
--spacing-xs: 4px
--spacing-sm: 8px
--spacing-md: 12px
--spacing-lg: 16px
--activity-bar-width: 48px
```

### Typography
```
--font-sans: system-ui, -apple-system, sans-serif (system font stack)
--font-mono: 'SF Mono', 'Cascadia Code', 'Fira Code', monospace
--font-size-xs: 11px
--font-size-sm: 12px
--font-size-base: 13px
--font-size-lg: 14px
```

## Atmosphere Effects (from `src/assets/styles/atmosphere.css`)

### Paper Grain Texture
```css
.paper-grain::before {
  /* SVG noise filter overlay */
  filter: url("data:image/svg+xml,...#noise");
  opacity: 0.03;  /* very subtle */
}
```

### Vignette Effect
```css
.paper-vignette::after {
  background: radial-gradient(ellipse at center, transparent 50%, rgba(0,0,0,0.06) 100%);
}
```

## UnoCSS Configuration (`uno.config.ts`)

### Theme Extensions
```js
colors: {
  bg: 'var(--color-bg)',
  surface: 'var(--color-surface)',
  border: 'var(--color-border)',
  text: 'var(--color-text)',
  accent: 'var(--color-accent)',
  // ... maps all CSS variables to UnoCSS theme
}
```

### Key Shortcuts
```
'glass-panel': 'bg-surface/80 backdrop-blur-sm border border-border-subtle rounded-md'
'card-base': 'bg-surface rounded-md shadow-sm border border-border-subtle'
'btn-primary': 'bg-accent text-white rounded-sm px-3 py-1 text-sm hover:bg-accent-hover'
'btn-ghost': 'text-text-secondary hover:bg-surface-hover rounded-sm px-2 py-1 text-sm'
'input-base': 'bg-surface border border-border rounded-sm px-2 py-1 text-sm'
```

### Shadow Definitions
```
'shadow-sm': '0 1px 2px var(--color-shadow-1)'
'shadow-md': '0 2px 8px var(--color-shadow-2)'  
'shadow-lg': '0 4px 16px var(--color-shadow-3)'
```

## Activity Bar Styling
- Fixed 48px width on left side
- Background: `var(--color-bg-secondary)`
- Icons: 20px, color `var(--color-text-tertiary)`, active: `var(--color-accent)`
- Active indicator: 2px left border in accent color
- Separator line on right edge: `var(--color-border-subtle)`

## Glass/Blur Effects
```
backdrop-blur-sm: blur(4px)
backdrop-blur-md: blur(8px)
backdrop-blur-lg: blur(16px)
```
Surface panels often use `bg-surface/80` (80% opacity) with backdrop blur for depth.

## Global Styles (`src/assets/styles/global.css`)
- Custom scrollbar styling (thin, warm-toned)
- Selection color: accent with 20% opacity
- Smooth transitions on interactive elements (150ms)
- Window drag region on title bar area

The overall aesthetic is warm, paper-like with brown-tinted shadows, ink green accents, brick red secondary accents, subtle grain texture, and a vignette effect — designed to feel like quality stationery rather than a cold tech interface.

</details>

两个 Agent 都回来了，信息非常充分。让我来给你构思方案。

---

## HTML 原型页构思

**目标**：一张截图，展示工作台的"全能力黄金状态"——让观众一眼看到这个工具能做什么。

### 布局结构（从左到右）

**① Activity Bar（48px）** — 6 个图标，"工作台"高亮

**② MonitorRail（~220px）** — 4 张监控卡片，各代表一种状态：
- 卡片 1：**流式进行中**（绿色脉冲点），显示 token 用量 + 费用，标题如 "重构用户认证模块"
- 卡片 2：**等待授权**（橙色警告），需要工具审批，标题如 "数据库迁移脚本"
- 卡片 3：**已完成**，显示完整 token 统计
- 卡片 4：**另一个活跃会话**

**③ Tab 栏** — 3 个 Tab（"后端重构"、"前端优化"、"Bug 修复"），第一个激活，右侧有 RunConfig 胶囊显示 "Claude Sonnet 4 · high"

**④ 主内容区（双列）** — 这是重点，左右分栏展示同一项目的两个会话：

**左列**（宽 60%）— 一个典型的对话流：
- 用户消息："帮我重构这个认证模块，需要支持 OAuth2"
- 助手回复包含：
  - 折叠的思考块（显示 "思考 · 1,247 tokens"）
  - 一段简短的分析文本
  - 一个展开的工具调用（Read 文件）
  - 一个展开的工具调用（Edit 文件，显示 diff）
  - 一个折叠的子 Agent 块（"Agent: 搜索 OAuth2 最佳实践"）
  - 正在流式输出的文本（末尾有光标闪烁）

**右列**（宽 40%）— 另一个会话，展示不同能力：
- 一个 Bash 工具调用的结果（测试输出）
- 代码块（语法高亮）

### 关键展示点清单

| 能力 | 在截图中的体现 |
|------|--------------|
| 多会话监控 | MonitorRail 4 张卡片 |
| 实时流式 | 脉冲点 + 正在输出的文本 |
| 多列并排 | 左右分栏 |
| Tab 多上下文 | 3 个 Tab |
| 思考块 | 折叠态，显示 token 数 |
| 工具调用 | Read/Edit/Bash 多种 |
| Diff 展示 | Edit 工具的变更对比 |
| 子 Agent | 折叠的 subagent 块 |
| 权限请求 | MonitorRail 卡片的授权提示 |
| 模型/渠道选择 | RunConfig 胶囊 |
| Token/费用统计 | 卡片上的用量显示 |
| Paper 主题质感 | 暖色调、纸张纹理、墨绿点缀 |

### 一个担忧

塞太多信息到一张图里会显得拥挤，反而不像真实软件。我建议**做暗色模式**——暗色截图在社交媒体/开发者社区传播效果更好，也更容易藏住细节瑕疵。

你觉得这个构思方向可以吗？要调整哪些地方？暗色还是亮色？

完毕。