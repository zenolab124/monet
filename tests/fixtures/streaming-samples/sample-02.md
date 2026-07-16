现在我有足够的信息来撰写深度分析报告了。让我整理并生成最终的结构化报告：

## 深度分析报告：CLI-Manager 前端架构

### 1. 整体前端架构

#### 1.1 App.tsx 的路由与布局结构

**核心特点：**
- **启动阶段管理**：分阶段初始化（设置加载 → 依赖系统并行加载 → 项目列表 → 延迟启动任务）
- **两种视图模式**：
  - `standard` 模式：边栏 + 主内容区（终端标签 + 分屏视图）
  - `compact` 模式：侧边栏独占，主内容区为空（移动端优化）
- **事件驱动架构**：
  - Tauri 事件监听：`claude-hook-notification`（CLI 事件）、`subagent-transcript-append`（子 Agent 转录增量）
  - 系统通知与弹窗 Toast 双通道通知
- **核心组件组织**：
  - 导入的关键组件：`Sidebar`、`TerminalTabs`、`CommandPalette`、`SettingsModal`（lazy）、`StatsPanel`（lazy）
  - 顶级状态依赖：`settingsStore`、`projectStore`、`sessionStore`、`historyStore`、`terminalStore`

**关键决策：**
- 延迟加载 SettingsModal 避免首屏开销
- Hook 通知统一通过 `handleCliHookEvent` 路由（支持子 Agent 转录自动分屏）
- 窗口关闭行为动态配置：最小化/直接退出/弹窗询问

---

### 2. Stores 状态管理模式与职责划分

#### 2.1 核心 Stores 架构

| Store | 行数 | 职责 | 关键状态 |
|-------|------|------|--------|
| **terminalStore.ts** | 1815 | 终端会话生命周期、PTY 通信、分屏管理、子 Agent 转录 | `sessions[]`、`paneTree`、`activeSessionId`、`tabNotifications` |
| **sessionStore.ts** | 83 | 会话持久化（Tauri Store） | `sessions[]`、`splits[]`、`activeSessionId`、`loaded` |
| **settingsStore.ts** | 857 | 应用配置、主题、快捷键、通知策略 | 主题、字体、键盘快捷键、Hook 配置目录、系统通知设置 |
| **historyStore.ts** | 1450 | 会话回看、搜索、统计分析、缓存管理 | `sessions[]`、`activeSession`、`stats`、`searchHits`、LRU 缓存 |
| **ccusageStore.ts** | 246 | 实时 Token 统计（通过 cc-usage 工具） | `report`、`toolStatus`、内存/DB 缓存 |
| **projectStore.ts** | 440 | 项目树、CRUD、搜索、供应商覆盖（cc-switch） | `projects[]`、`tree[]`、`providerBadges` |
| **terminalPaneTree.ts** | 378 | 分屏布局算法（不可变树结构） | `TerminalPaneNode`（叶/分割节点）、分割比例 |
| **replayStore.ts** | 643 | 会话重放、事件记录（DB 存储） | `sessions[]`、`eventsBySession` |
| **gitStore.ts** | 680 | Git 变更、Diff、分阶段提交 | `changes[]`、`tree[]`、`selectedUntracked`、`deselectedAdded` |
| **fileExplorerStore.ts** | 822 | 项目文件浏览、编辑、搜索 | `tree[]`、`activeFile`、`openFiles[]`、`gitChanges[]` |

#### 2.2 状态管理设计模式

**Zustand 模式细节：**
```
terminalStore:
  ├─ 非持久化状态（内存）：sessions、paneTree、statusListeners
  ├─ 状态分层导出：tabNotifications（基于 CLI hook 事件）
  ├─ 持久化桥接：通过 sessionStore 异步保存/恢复
  └─ 事件驱动更新：handleCliHookEvent、handleShellRuntimeEvent

settingsStore:
  ├─ 自动持久化：Store.set() 配 autoSave=100ms
  ├─ 主题级联计算：resolvedTheme（system → dark/light）
  ├─ 合约类型导出：LightThemePalette、DarkThemePalette
  └─ 系统事件监听：matchMedia('prefers-color-scheme')

historyStore:
  ├─ 多层缓存策略：
  │  ├─ 内存 LRU（stats）：容量 16，TTL 5min
  │  ├─ SQLite（sessions/prompts/stats）：主查询源
  │  └─ 搜索缓存：按 query 的结果分页
  ├─ 分页加载：session 100/页，message 160/页
  └─ 元数据管理：alias、starred、tags（独立表）
```

**关键状态流向：**
- Hook 事件 → `terminalStore.tabNotifications` → UI Toast + 系统通知
- 会话 CRUD → `terminalStore.sessions` → `sessionStore` 异步持久化
- 搜索查询 → `historyStore.runGlobalSearch()` → SQLite 全文搜索结果缓存

---

### 3. 关键组件架构

#### 3.1 XTermTerminal.tsx（2044 行）- PTY 通信与实时统计

**核心职责：**
- **PTY 输出流处理**：
  - 原始 PTY 流 → OSC 序列解析 → xterm 渲染
  - Shell integration 支持：133/633（VS Code 标准）、777（本应用私有）
  - 后台 Tab 输出缓冲：`INACTIVE_BUFFER_MAX_CHARS` = 8MB，防止内存溢出

**实时统计机制：**
```
OSC 序列处理：
  ├─ LEGACY_RUNTIME_OSC_PREFIX = "\x1b]777;cli-manager;"
  ├─ extractOscSequence(chunk) → { kind, prefix, payload }
  ├─ 状态事件（command_started/finished/prompt_shown）→ shellRuntimeEvent
  └─ 路由：useTerminalStore.handleShellRuntimeEvent()
```

**关键数据结构：**
- `ACTIVE_WRITE_FRAME_BUDGET = 64KB`：活跃 Tab 每帧写入字节限制
- `SearchResultState`：搜索高亮状态（当前/总数）
- `OscPrefixMatch`：OSC 序列前缀匹配器（处理跨 chunk 字符串）

**Hook 集成：**
```tsx
useEffect(() => {
  listen("pty-data", (event) => {
    // 原始数据写入 xterm buffer
    terminal.write(event.payload);
    // 并行：OSC 序列提取 → 状态事件触发
    extractAndDispatchShellRuntimeEvent(event.payload);
  });
  return unlisten;
}, [sessionId]);
```

**性能优化：**
- `WebglAddon`：GPU 加速渲染
- `ImageAddon`：限制内存（4MB 像素 + 8MB 序列）
- `FitAddon`：自适应尺寸，监听容器 ResizeObserver
- Unicode 11 支持（Emoji 等）

#### 3.2 SplitTerminalView.tsx（201 行）- 不可变分屏树

**分屏数据结构：**
```
TerminalPaneTree（树形）:
  ├─ TerminalPaneLeaf
  │  ├─ id: 唯一 pane ID
  │  ├─ sessionIds[]：该 pane 的 Tab 列表
  │  └─ activeSessionId：当前活跃 Tab
  └─ TerminalPaneSplit
     ├─ direction："horizontal" | "vertical"
     ├─ ratio：分割比例（0.2～0.8，clamped）
     ├─ first, second：子树（递归）
     └─ id：分割节点 ID
```

**布局算法（buildSplitLayout）：**
- 递归分解树 → 叶片矩形集合 + 分割线矩形集合
- ResizeObserver 监听容器变化 → 重新计算布局
- 拖拽分割线时保存预览 state，松开后调用 `setSplitRatio()`

**拖拽交互：**
```tsx
const onDragStart = (split, splitRect, e) => {
  const onMove = (ev) => {
    latestRatio = clampSplitRatio(
      isHorizontal 
        ? (ev.clientX - splitRect.left) / splitRect.width
        : (ev.clientY - splitRect.top) / splitRect.height
    );
    requestAnimationFrame(flush);
  };
  document.addEventListener("mousemove", onMove);
  // 松开时提交到 store
};
```

#### 3.3 HistoryWorkspace.tsx（698 行）- 会话回看与深度搜索

**双侧板布局：**
- 左侧栏（可调宽度）：会话列表 + 全局搜索
- 右侧栏：会话详情 + 消息搜索 + Diff 预览 + Prompt 库

**搜索能力：**
```
globalQuery（全局搜索）：
  ├─ 命中范围：session 标题、消息内容、标签
  ├─ 限制：120 条结果、实时高亮
  └─ 缓存：按 query 分组

sessionQuery（当前会话内搜索）：
  ├─ 命中范围：消息索引 + 消息内容
  ├─ 分页：160 消息/页，自动加载
  └─ 高亮：搜索词标记位置
```

**会话恢复流程：**
1. 获取 resume 命令：`claude --resume {sessionId}` 或 `codex resume --no-alt-screen`
2. 从 `historyStore.activeSession` 推导 CWD（优先级：session.cwd → project.path）
3. 调用 `terminalStore.createSession()` 创建新 PTY
4. 挂载 resume 命令作为 startupCmd

**性能优化：**
- 列表虚拟化：SESSION_PAGE_SIZE=100，load-more-threshold=220px
- 自动展开会话消息：AUTO_OPEN_SESSION_DELAY_MS=180
- Diff 解析Worker：后台异步（diffParser.worker.ts）

#### 3.4 CommandPalette.tsx（315 行）- 命令调度系统

**Palette 数据结构：**
```
PaletteItem:
  ├─ id: 唯一标识符
  ├─ label: 显示名称
  ├─ category: 分组（"操作"、"项目"、"模板"）
  ├─ description: 简述
  └─ action: 回调函数

PaletteRow = { type: "header" | "item" }
```

**Fuzzy 匹配算法：**
```tsx
function fuzzyMatch(text, queryLower): boolean {
  let qi = 0;
  for (let i = 0; i < text.length && qi < queryLower.length; i++) {
    if (text[i] === queryLower[qi]) qi++;
  }
  return qi === queryLower.length; // 所有查询字符依序出现
}
```

**动态项集生成：**
- 操作类：新建、分屏、合并、主题切换
- 项目类：按 active session 推导 projectId，列出所有项目
- 模板类：按上下文（projectId/sessionId）过滤命令模板

**使用 zustand store 的特殊用法：**
```tsx
export const useCommandPaletteStore = create<{ isOpen, open, close, toggle }>(...);
// 在 keyboard shortcuts hook 中直接调用
useCommandPaletteStore.getState().toggle();
```

#### 3.5 SettingsModal 多页签结构

**页签定义（9 个）：**
| 页签 | 组件 | 职责 |
|-----|------|------|
| general | GeneralSettingsPage | 语言、主题、字体、通知策略 |
| sidebar | SidebarSettingsPage | 侧栏宽度、密度、工具栏可见性 |
| terminal-theme | ThemeSettingsPage | 终端主题选择、背景、滚回行数 |
| shortcuts | ShortcutSettingsPage | 键盘快捷键编辑、冲突检测 |
| templates | TemplateSettingsPage | 命令模板 CRUD |
| providers | ProviderSettingsPage | Codex 供应商配置（cc-switch 集成） |
| model-pricing | ModelPricingSettingsPage | Token 价格表、模型列表 |
| sync | SyncSettingsPage | 云同步配置（S3/云存储） |
| hooks | HookSettingsPage | Claude Code / Codex Hook 目录配置、安装状态 |
| about | AboutSettingsPage | 版本、更新检查、许可证 |

**导出的 SettingsTab 类型**：
```tsx
export type SettingsTab = 
  | "general" | "sidebar" | "terminal-theme" | "shortcuts" 
  | "templates" | "providers" | "model-pricing" | "sync" 
  | "hooks" | "about";
```

---

### 4. Hooks 自定义钩子

#### 4.1 useKeyboardShortcuts.ts

**快捷键映射（8 个）：**
```ts
type ShortcutAction = 
  | "newTerminal" (Ctrl+Shift+T)
  | "closeTerminal" (Ctrl+W)
  | "nextTab" (Alt+ArrowRight)
  | "prevTab" (Alt+ArrowLeft)
  | "commandPalette" (Ctrl+P)
  | "sessionHistory" (Ctrl+K)
  | "copyAi" (Alt+P)
  | "toggleTerminalFullscreen" (F11);
```

**上下文感知处理：**
```tsx
const isEditingTarget = tag === "INPUT" || tag === "TEXTAREA" || contenteditable;
const isXtermTarget = !!target?.closest(".xterm");

// 编辑状态下跳过全局快捷键
if (isEditingTarget && !isXtermTarget) return;
```

**按钮映射检测（鼠标 button 3/4）：**
- 按钮 3（侧后）：前一个 Tab
- 按钮 4（侧前）：后一个 Tab

#### 4.2 useFocusTrap.ts

用于 Modal/Dialog 内的焦点管理，支持 Tab 循环和 Escape 关闭。

---

### 5. 工具库架构

#### 5.1 terminalThemes.ts - 主题系统

**主题组织（6 个分组）：**
```ts
export type TerminalThemeGroupId = 
  | "cool" (蓝紫调)
  | "warm" (复古棕橙)
  | "nature" (自然绿)
  | "pink-purple" (柔和粉紫)
  | "high-contrast" (高对比度)
  | "light-office" (亮色办公);

export interface TerminalThemePreset {
  id: string;
  name: string;
  theme: ITheme; // xterm ITheme 结构
  group: TerminalThemeGroupId;
  tone?: "light" | "dark";
}
```

**亮度检测算法（WCAG 相对亮度）：**
```ts
function getRelativeLuminance([r, g, b]): number {
  const [srgbR, srgbG, srgbB] = [r, g, b].map(c => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * srgbR + 0.7152 * srgbG + 0.0722 * srgbB;
}

export function isLightTerminalTheme(theme: ITheme): boolean {
  return getRelativeLuminance(hexToRgb(theme.background)) > 0.55;
}
```

**应用：** 确定叠加层颜色、对比度要求等

#### 5.2 providerSwitching.ts - 供应商覆盖管理

**Codex 供应商配置结构：**
```ts
export interface CodexProviderOverride {
  providerId: string;
  providerName: string | null;
  profileName: string;
  vendorHint?: string | null; // 工具提示
}

export interface ProjectProviderOverrides {
  codex?: CodexProviderOverride;
}

// 序列化：Project.provider_overrides（JSON string）
// 解析：parseProjectProviderOverrides(raw: string)
```

**关键函数：**
- `getCodexProviderOverride(project)`：从项目获取 Codex 配置
- `getProviderSwitchAppType(project)`：判断 CLI 工具（claude/codex）
- `withCodexProviderOverride(raw, override)`：更新覆盖配置

#### 5.3 diffParser.ts / diffParser.worker.ts - Diff 解析

**支持格式：**
- unified diff（标准 git）：`diff --git a/... b/...`
- apply-patch 格式：`*** Begin Patch ... *** End Patch`
- 代码块围栏：```` ```diff ... ``` ````

**解析输出：**
```ts
export interface ParsedDiffBlock {
  id: string;
  filePath: string;
  patch: string;
  messageIndex: number;
  timestamp: string | null;
}
```

**Worker 模式：** 后台线程异步解析，避免阻塞 UI

#### 5.4 i18n.ts - 国际化系统

**两语言支持：**
```ts
export type AppLanguage = "zh-CN" | "en-US";
export type LanguagePreference = "auto" | "zh-CN" | "en-US";

export function resolveLanguagePreference(lang: LanguagePreference): AppLanguage {
  return lang === "auto" ? detectPreferredLanguage() : lang;
}
```

**翻译 Key 结构：**
- `sidebar.projects`、`sidebar.newTerminal`、`history.empty.noHistoryTitle`
- 支持参数化：`{count}`、`{time}`、`{tabTitle}`

**Hook 用法：**
```tsx
const { language, t } = useI18n();
// t("key.name") 获取当前语言的翻译
// 自动响应 settingsStore.language 变化
```

#### 5.5 其他关键库

| 库 | 职责 |
|---|------|
| shell.ts | Shell 类型检测（bash/zsh/fish/powershell）、默认 Shell 平台检测 |
| contrast.ts | 颜色对比度计算（WCAG AA/AAA） |
| projectStartupCommand.ts | Codex TUI 主题、启动命令解析 |
| diffParser.ts | Git diff / apply-patch 格式解析 |
| aiClipboard.ts | AI 路径块复制（Ctrl+Alt+P） |
| aiPathFormatter.ts | `ai://` 协议格式化 |
| logger.ts | 性能打点、错误日志、控制台输出 |
| db.ts | SQLite 连接管理、事务、批量操作 |
| queryClient.ts | React Query 客户端配置（缓存策略） |
| modelPricing.ts | Token 价格计算 |
| monacoSetup.ts | Monaco Editor 初始化（Diff 查看器） |
| externalTerminal.ts | 外部终端启动（Windows CMD/PowerShell） |
| assetUrl.ts | 资源 CDN 地址解析 |
| systemFonts.ts | 系统字体列表枚举 |
| terminalCloseConfirm.ts | Terminal Tab 关闭确认事件 |
| terminalFileDrag.ts | 终端拖拽文件支持 |

---

### 6. 样式组织方式

**文件结构（4 个 CSS 文件）：**
```
styles/
├─ base.css          # CSS 变量、重置、基础排版
├─ themes.css        # 主题调色板（light/dark palette + terminal theme）
├─ components.css    # UI 组件样式（Button、Input、Dialog 等）
└─ animations.css    # 动画、过渡（slide、fade、spin）
```

**CSS 变量分层：**
```css
/* 主题级 */
--bg-primary, --bg-secondary
--text-primary, --text-secondary, --text-muted
--border-primary, --border-muted

/* 排版 */
--font-ui-sans, --font-ui-mono
--font-size-ui, --font-size-body, --font-size-meta
--line-height-body, --line-height-compact

/* 终端 */
--terminal-font-family
--terminal-scrollback-rows (动态)
```

**数据属性驱动主题：**
```html
<html 
  data-theme="dark" 
  data-light-palette="warm-paper"
  data-dark-palette="night-indigo"
  lang="zh-CN"
/>
```

**响应式方法：** Tailwind CSS 工具类 + CSS 变量覆盖（不同主题切换）

---

### 7. 用户体验亮点

#### 7.1 多通道通知系统
- **应用内 Toast**（Sonner）：即时反馈、可自动关闭
- **系统通知**（Tauri native）：后台 Tab 事件警报
- **Hook 弹窗 Toast**：4 种变体（attention/approval/finished/failed）、自动关闭倒计时配置

#### 7.2 渐进式体验
- **首屏加载**：仅加载 settingsStore，其他 store 并行加载
- **延迟初始化**：settings modal/stats panel 首次打开时才创建
- **性能打点**：`createPerfMarker("app.first_screen")` 记录首屏时间

#### 7.3 分屏体验
- **不可变树结构**：状态变化后生成新树副本，易于 redo/undo
- **拖拽分割线**：边拖边预览，释放时持久化
- **快捷键支持**：`Alt+ArrowRight/Left` 切换 Tab，`Ctrl+P` 打开命令面板

#### 7.4 会话搜索与恢复
- **全文搜索**：SQLite FTS（session title + message content）
- **高亮导航**：搜索结果支持直接打开指定消息
- **一键恢复**：自动检测项目路径，创建新 PTY 并运行 resume 命令

#### 7.5 智能主题系统
- **跟随系统**：`matchMedia('prefers-color-scheme')` 监听系统深浅色
- **终端独立主题**：可设置终端与 UI 主题分离
- **对比度自适应**：亮/暗终端背景自动调整叠加层和文本颜色

#### 7.6 快捷键定制
- **冲突检测**：设置中实时验证快捷键冲突
- **上下文感知**：编辑区内禁用全局快捷键
- **鼠标按钮支持**：侧后/侧前按钮切换 Tab

---

### 8. 架构决策与权衡

#### 8.1 状态管理决策
- **选择 Zustand**：轻量级、无 boilerplate、支持异步 action、易集成 Tauri Store
- **分 Store 策略**：
  - 业务 Store（terminal/project/history）：通常较大，做细粒度功能划分
  - UI Store（settings）：集中管理全局 UI 配置，避免 prop drilling

#### 8.2 持久化策略
- **分层缓存**：
  - 内存 LRU（热数据）
  - SQLite（冷数据历史）
  - Tauri Store（用户配置）
- **自动保存**：Tauri Store 配 autoSave 防止丢失

#### 8.3 性能优化
- **组件拆分**：TerminalTabs（2555 行）、XTermTerminal（2044 行）职责明确，但单文件大
- **虚拟化**：历史列表长列表虚拟化，分页加载
- **Worker**：Diff 解析异步化避免卡顿
- **缓存策略**：热点数据（stats）内存 LRU，冷数据查询结果缓存

#### 8.4 多语言与国际化
- **集中管理**：i18n.ts 导出全量翻译对象
- **参数化翻译**：支持动态插值（格式 `{key}`）
- **自动检测**：navigator.language/languages 推导默认语言

#### 8.5 通知策略
- **分类配置**：系统通知可按事件类型关闭（SessionStart/Stop/PermissionRequest 等）
- **自动关闭**：可配置 Hook 弹窗自动关闭倒计时
- **焦点恢复**：系统通知点击后自动聚焦应用窗口

---

### 9. 关键文件路径速查表

**核心应用：**
- `/src/App.tsx`：主应用组件、事件处理、窗口管理
- `/src/main.tsx`：React 启动入口

**Stores（状态管理）：**
- `/src/stores/terminalStore.ts`：会话/分屏/通知管理
- `/src/stores/settingsStore.ts`：配置持久化
- `/src/stores/historyStore.ts`：会话检索/统计
- `/src/stores/projectStore.ts`：项目树管理
- `/src/stores/terminalPaneTree.ts`：分屏树算法

**主要组件：**
- `/src/components/XTermTerminal.tsx`：xterm.js 集成、OSC 序列处理
- `/src/components/SplitTerminalView.tsx`：分屏布局与拖拽
- `/src/components/HistoryWorkspace.tsx`：会话回看
- `/src/components/CommandPalette.tsx`：命令调度
- `/src/components/TerminalTabs.tsx`：Tab 管理
- `/src/components/SettingsModal.tsx`：多页签设置

**子组件目录：**
- `/src/components/settings/pages/`：9 个设置页
- `/src/components/stats/`：实时/历史统计面板
- `/src/components/history/`：会话详情、搜索、Diff 查看器
- `/src/components/sidebar/`：项目树、项目操作菜单
- `/src/components/git/`：Git 变更面板、提交对话框
- `/src/components/files/`：项目文件浏览器

**工具库：**
- `/src/lib/terminalThemes.ts`：主题与亮度检测
- `/src/lib/providerSwitching.ts`：Codex 供应商配置
- `/src/lib/diffParser.ts`：Diff 格式解析
- `/src/lib/i18n.ts`：国际化翻译
- `/src/lib/shell.ts`：Shell 类型识别

**Hooks：**
- `/src/hooks/useKeyboardShortcuts.ts`：快捷键绑定与处理
- `/src/hooks/useFocusTrap.ts`：Modal 焦点管理

**样式：**
- `/src/styles/base.css`：基础变量与重置
- `/src/styles/themes.css`：主题配色
- `/src/styles/components.css`：UI 组件样式
- `/src/styles/animations.css`：动画定义

---

### 10. 总结：架构思路与设计优势

**核心设计哲学：**
1. **事件驱动的异步架构**：Tauri 事件 → Store 更新 → UI 响应，清晰的数据流向
2. **分层状态管理**：Zustand 多 store 而非大 store，易于功能拆分和测试
3. **持久化首选**：离线优先设计，SQLite 本地缓存，支持冷启动快速恢复
4. **渐进式 UX**：延迟加载、虚拟化、缓存策略，首屏快速，功能探索平滑

**技术栈优势：**
- **Tauri + React**：轻量级跨平台桌面应用，Rust 后端高效通信
- **Zustand**：极简状态管理，与 Tauri Store 集成天然
- **xterm.js**：成熟 Web 终端组件，支持 GPU 加速 + OSC 序列扩展
- **Mantine UI + Tailwind**：一致性设计系统，快速原型和定制

**可扩展性考量：**
- Plugin 架构预留：Command Palette 支持运行时注册新命令
- 国际化框架：支持轻松添加新语言
- 主题系统：不可变主题数据结构，易于新增主题
- Store 插件化：可通过自定义 store 注入新功能

这个 CLI-Manager 前端是一个设计成熟的、功能完整的终端管理系统，展现了现代前端应用如何通过分层架构、事件驱动和精细化缓存策略来构建高效、可响应的桌面应用。