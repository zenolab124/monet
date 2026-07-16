完美。现在我已经有了全部信息。让我生成知识库更新方案。

## 知识库条目更新方案

根据本次布局重构变更，以下是受影响的知识库条目的完整更新内容：

---

## 1. docs/knowledge/domains/app-shell.md

```markdown
# 应用壳：TitleBar + ActivityBar + 域切换

应用根架构：Vue 3 单页 + Tauri WebView，无路由。v2.0.0（FR-006）起壳 = 左侧 ActivityBar + 域视图 `v-show` 切换，状态全部走 composable 单例；v2.1.0 点亮工作台域、三栏只读化为档案馆，壳层另挂全域通知 toast 与全局确认弹窗；**v2.5.0 加入 TitleBar（32px 全宽标题栏），纵向壳改为 TitleBar + (ActivityBar + 视图) 横向布局**。

## 入口

- `src/main.ts:1-8` — `createApp(App).mount('#app')`，样式引入链：UnoCSS → `styles/paper/paper.css`（token 真理源）→ `extends.css`（项目扩展）→ `prose.css`（详见 [`style-system.md`](../shared/style-system.md)）
- `src/App.vue` — 顶层纵向 flex：`<TitleBar />` + `<ActivityBar />` + 六个域视图（Workbench / Sessions / Workshop / Automation / Home / Settings），域外另挂全域可见的 `<ToastStack />`（通知层）与 `<ConfirmDialog />`（全局确认）

## TitleBar（v2.5.0 新增）

`src/components/TitleBar.vue:15-43`：32px 固高全宽标题栏，分为四段：

- 左边 78px 拖拽区（macOS 红绿灯避让，`data-tauri-drag-region`）
- 左 slot（`#leading`）：内容由 activeSection 驱动——WorkbenchView 域显示 `WorkbenchTabs`（tab 管理条），其他域显示域名文字（「档案」/「工坊」/「自动化」/「首页」/「设置」）
- 中间弹性拖拽区（`data-tauri-drag-region`）
- 右 slot（`#trailing`）：`TitleBarTools` 组件，按 activeSection 渲染各域工具按钮（搜索 + 侧栏折叠 / 刷新 / 打开配置等）

**为什么不用 Teleport？** TitleBar 的 slot 机制是为了避免组件生命周期复杂性。Teleport 需要 DOM 常驻且会跨越组件树边界，slot 插槽方案则在 App 层直接决定渲染什么，逻辑简洁且响应性集中在顶层 activeSection 上。

关键选项与样式：
- `titlebar` class：`height: 32px; flex-shrink: 0; display: flex; align-items: center; border-bottom: 1px solid var(--border); background: var(--secondary);`
- leading / trailing 均通过 flexbox 内陆对齐，leading 用 `min-w-0` 防缩写，trailing 用 `gap-1.5` 与 `pr-3` 紧凑右侧工具

## TitleBarTools（v2.5.0 新增）

`src/components/TitleBarTools.vue:1-112`：按 activeSection 条件渲染，共四个分支：

1. **sessions（档案馆）**：侧栏折叠按钮 + 会话搜索框（`⌘F` 快捷键，搜索框 `w-48 pl-7 pr-2 py-1` 尺寸）
2. **home（首页）**：刷新按钮（`i-carbon-renew`，loading 时 `animate-spin`）
3. **automation（自动化）**：打开配置 + 刷新双按钮；打开失败提示「打开失败」3 秒消失
4. **其他域**：无工具按钮显示

按钮样式统一为 `tb-btn` class：`display: inline-flex; gap: 4px; font-size: 11px; padding: 2px 8px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--card);` hover 显 `var(--shadow-paper)`；disabled 时 opacity 0.5。

## 域切换（v-show 而非路由）

`App.vue` 用 `v-show` 切换六个域视图（WorkbenchView / SessionsView / WorkshopView / AutomationView / HomeView / SettingsView）。**刻意 DOM 常驻**：流式打字机、滚动位置、事件监听在切换间零丢失（PRD 硬指标）——引路由或 `v-if` 都会重建组件树，故不引。工作台域内部再做一层渲染分级（非激活 tab 零 DOM），见 [`workbench.md`](./workbench.md)。

`useUiState.ts:8` 的 `activeSection: 'workbench' | 'sessions' | 'home' | 'settings' | 'workshop' | 'automation'`（v2.1.0 加 workbench、多渠道迭代点亮 settings、v2.3.0 加 workshop、v2.4.0 加 automation），与 `sidebarsCollapsed` 一起持久化进同一 localStorage key `cc-space-ui`，重启恢复上次所在域，非法值回退 `sessions`（validSections 白名单校验）。

## ActivityBar 七域（v2.5.0 变更：去掉 h-9 spacer，加回 data-tauri-drag-region）

`ActivityBar.vue:19-26` 终态七域全摆：工作台 / 档案 / 搜索 / 工坊 / 自动化 / 首页，底部另有外观切换 + 设置。逐版点亮——v2.1.0 工作台 + 「会话」更名「档案」、多渠道迭代设置、v2.3.0 工坊、v2.4.0 自动化。**至 v2.4.0 仅剩「搜索」一域 `disabled`**（`opacity 0.35` + tooltip「即将推出」，`ActivityBar.vue:110-114`）；点亮判据 = `topDomains` 项是否带 `section` 字段。

布局变更：
- v2.5.0 **去掉顶部 `h-9` 拖拽区**（之前为 macOS 红绿灯避让），改由 TitleBar 接手（TitleBar 内有 78px 拖拽占位）
- **恢复 `data-tauri-drag-region` 标记**到 ActivityBar 整体与弹性填充（`ActivityBar.vue:42, 63`），与 TitleBar + WorkbenchTabs 一起形成上半区完整拖拽
- 选中态 =「纸片拈起」（`ActivityBar.vue:94-109`）：card 底 + `--shadow-paper` + 左侧 2px primary 指示条
- 主题切换按钮从 Toolbar 移入底部（`ActivityBar.vue:68-70`）——首页域没有 Toolbar，放壳上才全域可达；Toolbar 只保留搜索框 + 侧栏折叠
- 工作台角标（v2.1.0 FR-007，`ActivityBar.vue:32-36`）：计数 = `useNotifications().badgeCount`（全部工作台未处理持久型事件：等权限 + 出错），0 隐藏、超 9 显「9+」；`accent` 单色不随事件类型变（`.ab-badge`，`ActivityBar.vue:115-129`），与 toast / 左列决策条同源

## 三栏（SessionsView 内）

原顶层三栏整体降级为 `src/views/SessionsView.vue`（终态语义为档案馆，v2.1.0 只读化）：项目侧栏 224px + 会话列表 288px + SplitView，交互不变。`sidebarsCollapsed` 整体收缩（`SessionsView.vue:17-18`），折叠动画 `width 220ms cubic-bezier(0.32, 0.72, 0, 1)` 贴合 macOS 抽屉手感（`SessionsView.vue:48-50`），折叠时清 `border-r` 防残线。

HomeView（`src/views/HomeView.vue`）v2.2.0 首页实化：三卡实数据（Token 消耗 / 兼容性诊断 / 活跃热力），数据源 `useHomeStats` 单例编排 `get_usage_stats` + `get_schema_diagnosis` 两个 IPC 调用。详见 [`home-dashboard.md`](./home-dashboard.md)。

## 全局快捷键

`App.vue:15-25` 监听 `keydown`：`Cmd/Ctrl+R` → `loadProjects()`；`Esc` → `selectSession(null)`。`Cmd/Ctrl+F` 聚焦搜索框定义在 `TitleBarTools.vue:20-27`。

## 红绿灯布局（v2.5.0 拍板定案）

macOS Overlay 标题栏红绿灯约 78px 宽，会压住 ActivityBar（48px），v2.5.0 拍板方案集中化：

- **TitleBar 左占位 78px**（`TitleBar.vue:17`），挤出 ActivityBar、给红绿灯留空
- ActivityBar 整条 nav 及其 flex-1 填充挂 `data-tauri-drag-region`（`ActivityBar.vue:42, 63`）
- WorkbenchTabs 的最后弹性区挂 `data-tauri-drag-region`（`WorkbenchTabs.vue:192`）
- 三者形成**连续拖拽条**从顶到底

`@contextmenu.prevent`（`App.vue:37`）禁用浏览器原生右键菜单，改用 Tauri 原生 Menu API（见 [`session-list.md`](./session-list.md)）。

## 背景与氛围层

窗体为不透明纸底 `bg-background`（毛玻璃已随 Paper 换肤整体下线，见 [`theming.md`](./theming.md)）。颗粒 + vignette 氛围层 `paper-atmosphere` 挂 body（`index.html:8`），**全站仅此一处**，z-index 口径见 [`style-system.md`](../shared/style-system.md)。

## 关联条目

- 项目侧栏数据流：[`project-discovery.md`](./project-discovery.md)
- 会话列表交互：[`session-list.md`](./session-list.md)
- 主题切换：[`theming.md`](./theming.md)
- 样式系统 / Paper 体系：[`../shared/style-system.md`](../shared/style-system.md)
- 三原语布局决策（档案馆/工作台/通知层）：[`../decisions/three-primitives-layout.md`](../decisions/three-primitives-layout.md)
- 状态选型：[`../decisions/no-pinia-composables.md`](../decisions/no-pinia-composables.md)
- 工坊域（v2.3.0 点亮）：[`./workshop.md`](./workshop.md)
- 自动化域（v2.4.0 点亮）：[`./automation.md`](./automation.md)
- 新增一个 ActivityBar 域的改动清单：[`../workflows/add-activitybar-domain.md`](../workflows/add-activitybar-domain.md)
```

---

## 2. docs/knowledge/domains/architecture-overview.md

```markdown
# 架构总览

CC Space 是 Claude Code 会话管理器，Tauri 2 双进程架构：Rust 后端做文件 / 进程 / 系统能力，Vue 3 前端做渲染 / 状态 / 交互。Mac 优先兼顾 Windows，迁移自 Swift 原版（见 [`swift-to-tauri.md`](../decisions/swift-to-tauri.md)）。

## 代码树

```
cc-space-tauri/
├── src/                    # Vue 3 前端（WebView 进程）
│   ├── components/         # UI 组件（TitleBar + TitleBarTools + ActivityBar + Toolbar + 三栏 + workbench/ 五组件 + notifications/ + 详情）
│   ├── views/              # 域视图（HomeView 首页 / SessionsView 档案馆三栏 / WorkbenchView 工作台 / WorkshopView 工坊 / AutomationView 自动化 / SettingsView 设置）
│   ├── composables/        # 全局单例状态 + 业务逻辑
│   ├── types/              # 与 Rust 端对齐的 TS 类型 + 工具函数
│   ├── styles/paper/       # Paper 主题 token 快照 + 项目扩展层
│   └── App.vue / main.ts   # 壳（TitleBar + ActivityBar + 域 v-show 切换 + ToastStack/确认弹窗挂根部）+ 入口
├── src-tauri/src/          # Rust 后端（主进程）
│   ├── commands.rs         # 13 个 #[tauri::command] 暴露给前端
│   ├── discovery.rs        # 扫描 ~/.claude/projects/（含目录名 → 路径贪心解码）
│   ├── parser.rs           # JSONL 解析
│   ├── streaming.rs        # 启动 claude CLI、流式事件、装配 MCP 网关（per-session 进程表）
│   ├── permission.rs       # Unix socket 权限服务（MCP 子进程 ↔ 前端的桥）+ stop_if_socket 竞态修复
│   ├── probe.rs            # schema-probe 共享核心（CLI bin + IPC command 双消费）
│   ├── usage_stats.rs      # 全量 JSONL token 聚合（rayon + message.id 去重）
│   ├── automation.rs       # Hook 配置读取 + 统计扫描 + Routine 调度（v2.4.0）
│   ├── workshop.rs         # 工坊资产扫描（Skills / Commands / Subagents / MCP，v2.3.0）
│   ├── bin/cc_space_mcp.rs # 独立二进制：--permission-prompt-tool MCP server
│   ├── bin/schema_probe.rs # 独立二进制：调 probe::run_probe() 的 CLI 入口
│   ├── watcher.rs          # 文件监控 + 1s 防抖 + 外部会话 api_error 增量探测
│   ├── cache.rs / tray.rs  # 缓存 + 托盘
│   └── models/             # Rust 端数据模型
└── docs/knowledge/         # 本知识库
```

`src/` 与 `src-tauri/src/` 之间通过 Tauri IPC 双向通信，类型契约定义在 `src/types/index.ts`，与 Rust 端 `models/` 模块字段一一对应（snake_case 直通，见 [`data-models.md`](../shared/data-models.md)）。

## 三原语布局（v2.1.0）

App.vue 壳（TitleBar + ActivityBar + 域 v-show 常驻，见 [`app-shell.md`](./app-shell.md)）之下，会话能力按「拉 / 交互 / 推」拆成三个原语（决策见 [`../decisions/three-primitives-layout.md`](../decisions/three-primitives-layout.md)）：

- **档案馆**（SessionsView 三栏）：只读浏览与检索——拉，你去找信息。详情区单面板（`SessionDetail mode="archive"`）无输入区/权限交互，流式中会话可只读跟看；底部只读条「在工作台打开」是进入交互的唯一出口（见 [`session-list.md`](./session-list.md)）
- **工作台**（WorkbenchView）：所有「活」会话的唯一交互场——多 tab × 左列监控卡 + 右区多列（`SessionDetail mode="workbench"` 完整交互）。核心心智：**在工作台 = 激活，与运行状态无关，进出全显式**；多会话并行流式由 per-session 进程表支撑（见 [`workbench.md`](./workbench.md)）
- **通知层**（ToastStack + ActivityBar 角标 + macOS 系统通知）：事件推送——推，信息来找你。权限/出错持久 toast 从事实源 computed 派生（与工作台监控卡决策条、列内权限卡三入口同源同步），「去会话」闭环跳回工作台（见 [`notifications.md`](./notifications.md)）

旧分屏系统（SplitView）随此布局下线，多会话并排归工作台多列（[`split-panes.md`](./split-panes.md) 保留作历史参考）。

## 数据流（典型路径）

```
用户操作（点击/快捷键）
  ↓
Vue 组件（components/*.vue）
  ↓
composable（composables/use*.ts，全局 ref 单例）
  ↓
invoke('<command>', args)         ←─ Tauri IPC
  ↓
Rust command（src-tauri/src/commands.rs 或独立模块）
  ↓
discovery / parser / streaming / permission(MCP gate) / 文件系统 / osascript / claude CLI
  ↓
Result<T, String> 反序列化为 TS 类型
  ↓
ref 更新 → Vue 渲染
```

反向通道：watcher / streaming 通过 `app.emit(event, payload)` 推事件，前端用 `listen()` 订阅（见 [`file-watching.md`](./file-watching.md)）。

## IPC 桥梁

**13 个 Commands**：`get_projects` / `get_session_records` / `get_session_summary` / `delete_session` / `resume_in_terminal` / `resume_in_vscode` / `start_streaming` / `stop_streaming` / `respond_permission` / `get_cli_settings` / `check_session_running` / `get_usage_stats` / `get_schema_diagnosis`。

**6 种 Events**：
- `projects-changed`：watcher 防抖 1s 后发，前端静默重载列表
- `stream-event`：流式消息片段（payload 为 `BlockStart | BlockDelta | BlockStop | AssistantMessage | Result | Error` enum；前三者是 `--include-partial-messages` 的字符级 SSE 透传，`AssistantMessage` 为终态快照兜底，见 [`streaming.md`](./streaming.md)）
- `stream-done`：流结束信号
- `permission-request`：claude CLI 想调用工具时，经 MCP 网关推送给前端的询问卡片（详见 [`./permission-request.md`](./permission-request.md)）
- ~~`permission-timeout`~~：已废弃（v2.2.0 移除自动超时，卡住等用户点）
- `session-api-error`：watcher 对**外部会话**（不在任何工作台）jsonl 新增内容做 api_error 增量探测后推给通知层（v2.1.0 FR-010 兜底；工作台内会话出错走流式链路，前端按 `findSession` 互斥去重，见 [`notifications.md`](./notifications.md)）

## MCP 权限网关旁路

`start_streaming` 会先起 `PermissionService` 监听 Unix socket，再用 `--mcp-config` 把 `cc-space-mcp` 二进制注册到 claude CLI：

```
claude CLI ↔(stdio JSON-RPC)↔ cc-space-mcp ↔(Unix socket)↔ 主进程 ↔(Tauri event)↔ 前端
```

工具调用前 claude CLI 先 `tools/call approve_tool_use`，主进程把请求 emit 到前端弹卡片，用户决策经 `respond_permission` command 反向打回。详见 [`./permission-request.md`](./permission-request.md) 与 [`../integrations/mcp-protocol.md`](../integrations/mcp-protocol.md)。

## 关联条目

- 入口与应用壳（TitleBar + ActivityBar + 域切换，三栏在 SessionsView 内）：[`app-shell.md`](./app-shell.md)
- 工作台（多 tab 监控与交互场）：[`workbench.md`](./workbench.md)
- 通知层（toast / 角标 / 系统通知）：[`notifications.md`](./notifications.md)
- 三原语决策（否决值守方案的背景）：[`../decisions/three-primitives-layout.md`](../decisions/three-primitives-layout.md)
- 项目侧栏：[`project-discovery.md`](./project-discovery.md)
- 会话列表：[`session-list.md`](./session-list.md)
- 会话操作：[`session-operations.md`](./session-operations.md)
- 文件监控：[`file-watching.md`](./file-watching.md)
- 主题切换：[`theming.md`](./theming.md)
- 数据模型契约：[`../shared/data-models.md`](../shared/data-models.md)
- 状态选型：[`../decisions/no-pinia-composables.md`](../decisions/no-pinia-composables.md)
- 权限请求链路：[`./permission-request.md`](./permission-request.md)
- MCP 协议：[`../integrations/mcp-protocol.md`](../integrations/mcp-protocol.md)
```

---

## 3. docs/knowledge/domains/automation.md

```markdown
# 自动化域

v2.4.0 自动化域 = Hooks 只读面板 + **本地定时任务（Routines）**。Hooks 侧：配置清单（全局 + 各项目 settings.json 的 hooks 字段）+ 近 7 天运行统计（从会话 JSONL 反推）。Routines 侧：本地 cron 调度 + CRUD 管理 + AI 自然语言解析，详见 [`./routines.md`](./routines.md)。

v2.5.0 布局调整：侧栏导航（Hooks / Routines 双 tab）纸片包裹，内容区 `content-area` 居中限宽。

## 架构

双数据源，并发拉取、前端关联；两 tab 侧栏导航（纸片）+ 右区内容（content-area 布局）：

```
AutomationView.vue (h-full p-2.5)
  └─ div.h-full.flex.bg-card.border.rounded-lg.shadow-paper
      ├─ nav.auto-nav (纸片内左侧导航 160px)
      │   ├─ auto-nav-title 「自动化」
      │   ├─ auto-nav-item (Hooks) active 态：card 底 + shadow-paper
      │   └─ auto-nav-item (Routines)
      └─ div.flex-1.min-w-0.overflow-y-auto (纸片内右区内容)
          └─ div.content-area (px-5 py-4)
              ├─ section[v-show="autoTab === 'hooks'"]
              │   ├─ 配置加载中 / 加载失败 / 空态
              │   └─ auto-table-wrap → auto-table (Hooks 列表)
              └─ section[v-show="autoTab === 'routines'"]
                  ├─ h2.sec-title + 操作按钮
                  └─ auto-table-wrap → auto-table (Routines 列表)

useAutomation.ts（模块级单例）
  ├── ensureLoaded() — loaded 守卫；并发 loadConfig() + loadStats()，配置先到先渲染
  ├── invoke('get_hooks_config')  → automation.rs（纯配置读取，含 homePath）
  ├── invoke('get_hooks_stats')   → automation.rs（rayon 扫近 7 天 JSONL）
  └── buildRows(entries, stats, statsLoading, homePath) — 按 (event, $HOME 归一化 command) 关联
```

**数据源不对称**：配置侧是结构化声明（settings.json 的 hooks 树），统计侧无中央日志——**hook 执行唯一的本地痕迹是会话 JSONL 里 `type=="attachment"` 且 `attachment.type=="hook_success"` 的记录**（带 `exitCode`），统计只能逐文件反推。两者用 `(event, command)` 关联，但配置写 `$HOME/...`、执行记录是展开后的绝对路径，必须归一化后才对得上（前后端各做一遍同款替换）。

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/automation.rs` | 三 command + 解析/扫描/ISO 解析全部数据层（461 行，无外部子模块） |
| `src/composables/useAutomation.ts` | 单例状态、并发加载编排、`buildRows` 关联、前端 `normalizeCommand` |
| `src/views/AutomationView.vue` | 视图根：纸片包裹的侧栏导航（Hooks / Routines 双 tab）+ 右区 content-area 内容；含分阶段加载/降级模板 |
| `src/components/TitleBarTools.vue` | `automation` 域工具按钮（打开配置 + 刷新） |
| `src/components/ActivityBar.vue` | `automation` 域图标点亮（`i-carbon-bot`） |
| `src/composables/useUiState.ts` | `AppSection` 含 `'automation'`，校验白名单 + localStorage 恢复 |
| `src/App.vue` | `AutomationView` v-show 接入（DOM 常驻） |

注：三个 command 注册在 `src-tauri/src/lib.rs` 的 `invoke_handler`（与 workshop 一类，**不在 `commands.rs`**——automation.rs 自带 `#[tauri::command]`）。

## 核心流程

- **配置先渲染、统计异步填充**：`ensureLoaded` 不 await，两个 invoke 并发。`config` 一到就出表格行，每行统计列在 `statsLoading && runs===null` 时显「…」，到货再替换。统计整体 `Err` 时降级——表格照常出配置，统计列显「统计不可用」/「—」。
- **$HOME 归一化关联**：`get_hooks_config` 返回 `homePath`；`buildRows` 把配置 command 做 `$HOME`/`${HOME}` → homePath 替换，再与 `get_hooks_stats` 已归一化的 command 拼 `(event\x00command)` key 匹配。统计 Rust 侧 `normalize_command` 与前端 `normalizeCommand` 两处实现必须一致。
- **rayon 并行扫描**：`collect_jsonl_files` 递归收集路径 → `par_iter().map(scan_jsonl_file)` 各线程产局部 `HashMap<StatKey, StatAccum>` → 主线程合并（runs/failures 累加、lastRun 取 timestamp 最大）。
- **mtime 粗筛 + timestamp 精筛**：先按文件 mtime 跳过 7 天前的文件（粗），单文件内再按记录 `timestamp` 二次过滤（mtime 新的文件仍可能含更早记录）。timestamp 用手写 `parse_iso_to_secs`（chrono 不引入，取前 19 字符 + Howard Hinnant 日历算法转 Unix 秒，7 天窗口判断够用）。跳过 `journal.jsonl`，含 `subagents/` 子会话文件（无害）。
- **聚合判定**：`exitCode==0` 计成功否则失败；`hook_additional_context` 无 exitCode 故天然被滤；timestamp 缺失的记录计入 runs/failures 但不参与 lastRun 评比。
- **双 tab 侧栏导航（v2.5.0）**：Hooks / Routines 分别渲染为 `auto-nav-item`，active 态同 ActivityBar 选中态 = card 底 + shadow-paper。点击切换 `autoTab` 本地状态（不持久化），两个 section 分别用 `v-show` 隐显。
- **content-area 布局（v2.5.0）**：`uno.config.ts` 新增 shortcut `content-area: 'max-w-280 mx-auto w-full'`（280 = 280px = `max-w-280` Tailwind 换算，上限 280px 宽、左右自动居中），替代前版逐组件手工 `max-w-xs` + `mx-auto`，统一内容区宽度与居中策略。AutomationView 右区 content-area 作用于 Hooks 表格和 Routines 区段。

## 视图布局细节

**外层壳（v2.5.0 纸片包裹）**：
```vue
<div class="h-full p-2.5">
  <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">
    <!-- 侧栏导航 -->
    <nav class="auto-nav"> ... </nav>
    <!-- 内容区 -->
    <div class="flex-1 min-w-0 overflow-y-auto">
      <div class="content-area px-5 py-4">
        <!-- Hooks / Routines 内容 -->
      </div>
    </div>
  </div>
</div>
```

相比 v2.4.0（无外层纸片，content 直接占满），v2.5.0 改为：
- 外层 `p-2.5` 留白（2.5 = 10px gap from container edge）
- 纸片 `bg-card border rounded-lg shadow-paper` 统一样式（与 HomeView / WorkshopView 一致）
- `overflow-hidden` 配合 `rounded-lg` 圆角裁切
- 内容区 `px-5 py-4` 内边距

## 关联条目

- [应用外壳](app-shell.md) — ActivityBar 七域壳 + TitleBar 工具按钮；automation 是第五个点亮域
- [Tauri 桥梁](../shared/tauri-bridge.md) — 全量 command/event 索引
- [Probe 模块](../shared/probe-module.md) — 同为「扫 JSONL + rayon 并行」模式的参照（probe 走 spawn_blocking + 共享 collect_jsonl；automation 是同模式独立重写，未复用其 collect_jsonl）
- [定时任务](./routines.md) — Routines 独立域（调度 + 持久化 + AI cron 解析）
- [Agent 服务层](./agent-service.md) — 自然语言转 cron 的 AI 能力
- [自动化只读决策](../decisions/readonly-config-domains.md) — 为什么只读不写 settings.json
- [hook 归一化坑](../pitfalls/hook-command-home-normalization.md) — $HOME 前后端归一化必须一致
- [样式系统 content-area](../shared/style-system.md) — content-area shortcut 的作用
```

---

## 4. docs/knowledge/domains/workshop.md

```markdown
# 工坊域

v2.3.0 新增的只读陈列域：把 Claude Code 的四类本地资产（Skills / Commands / Subagents / MCP 服务器）从全局 `~/.claude/` 与各项目目录扫出来，左子导航 + 右列表平铺，**纯展示**——无启停、无新建、无详情展开，唯一动作是「打开目录/配置」和「刷新」。MCP 的 http/sse 服务器额外做一次在线探活。

v2.5.0 布局调整：整体纸片包裹（bg-card），右区列表用 `content-area` 居中限宽；工坊导航加 background 色。

## 架构

```
ActivityBar「工坊」── v-show ──▶ WorkshopView.vue (h-full p-2.5)
                                   └─ div.h-full.flex.bg-card.border.border-border.rounded-lg.shadow-paper
                                       ├─ WorkshopNav (ws-nav 190px 左导航,四类+计数, background 色)
                                       └─ main.flex-1.min-w-0.overflow-y-auto (右区)
                                           └─ div.content-area.px-6.5.py-5 (限宽居中)
                                               ├─ AssetList → AssetItem  (skills/commands/agents, 双列 grid)
                                               └─ McpList   → AssetItem  (mcp,带探活/禁用徽章)
                                   状态: useWorkshop.ts (模块级单例,与 useHomeStats 同模式)

Rust:  workshop.rs (自带 command,不经 commands.rs)
   get_workshop_assets ─ spawn_blocking ─ collect_workshop_assets
        └ 装配: dirs::home_dir + discovery::discover_all() 项目 cwd 列表
             └ collect_assets(home, cwds)  ← 纯函数,单测注入 fixture
                  scan_skills_dir / scan_commands_dir / scan_agents_dir / scan_mcp_json
   probe_mcp_server(url) ─ reqwest GET (OnceLock 共享 Client,3s 超时)
   open_workshop_dir(category) ─ open/explorer/xdg-open
```

前端职责：左右两栏渲染、域内类别本地态（默认 Skills，不持久化）、MCP 探活编排（并发 invoke + 代次作废）。后端职责：文件系统扫描 + frontmatter/JSON 局部反序列化 + 排序 + http 探活。所有 command 返回 `rename_all = "camelCase"`（与项目多数 snake_case 直通的 command 不同，故前端类型 `mcpServers`/`argumentHint` 为驼峰）。

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/workshop.rs` | 数据层全部：四类扫描纯函数 + transport 推断 + frontmatter 解析 + 三个 command + 单测 |
| `src/composables/useWorkshop.ts` | 单例：惰性加载 + 内存缓存 + 刷新 + MCP 探活状态表 |
| `src/views/WorkshopView.vue` | 域根：纸片包裹（bg-card）+ 左导航 + 右区 content-area + 页头（打开/刷新）+ 加载/错误/列表分支 |
| `src/components/workshop/WorkshopNav.vue` | 四类子导航 + 计数（未回「…」/失败「—」）；v2.5.0 加 background 色 |
| `src/components/workshop/AssetList.vue` | Skills/Commands/Subagents 通用列表（双列 grid）；前端按 Rust 序直渲，不再排序 |
| `src/components/workshop/AssetItem.vue` | fitem 行公共壳，徽章走默认 slot |
| `src/types/index.ts` | `Workshop*` 驼峰类型（对齐 Rust camelCase 输出） |

## 核心流程

- **惰性加载**：`useWorkshop` 是模块级单例，`loadedOnce` 守卫。WorkshopView 用 `watch(activeSection, immediate)` 在「首次进入工坊（含启动即恢复到工坊）」触发 `ensureLoaded()`；会话期内存缓存，「刷新」才强制重调（防重入）。
- **扫描装配/纯函数分层**：command 只拼家目录 + `discovery::discover_all()` 的项目 cwd 列表（去重、剔空），核心 `collect_assets(home, cwds)` 是接受根路径的纯函数，单测注入 temp fixture。CPU/IO 扫描走 `spawn_blocking` 不阻塞异步运行时。
- **MCP 三来源**：① `~/.claude.json` 顶层 `mcpServers`（全局，恒 enabled）；② `projects.<cwd>.mcpServers`（local scope，`claude mcp add` 默认写入，无禁用机制恒 enabled）；③ `<cwd>/.mcp.json`（enabled = 名字不在该项目 `disabledMcpjsonServers` 中）。同名②③**不合并、各自成行**（path 不同 → 前端探活 key 不冲突）。`~/.claude.json` 用局部 struct 局部反序列化，敏感字段（env/args/headers）不声明即不进内存。
- **探活时机**：进入 MCP 子页（`watch([activeSection, category])`）或数据到达/刷新成功且停留在 MCP 子页（`watch(assets)`）触发 `probeMcpServers()`，覆盖「域内切到 mcp」「带 mcp 选中态切回工坊」「首屏数据未回」「刷新」四条路径。只对 http/sse 并发 GET，stdio 及其他 transport 不发请求、恒显「`<transport>` · 未探活」。`probeGen` 代次自增，旧轮回调用 `gen === probeGen` 作废，避免过期结果覆盖。探活结果不持久化、不轮询。
- **open_workshop_dir 兜底**：skills/commands/agents 目录不存在先 `create_dir_all` 再打开；mcp 打开家目录（`~/.claude.json` 所在，不创建）。spawn 后**不等退出码**（explorer 成功也常返非零）。失败时前端置 `openFailed` 显「打开失败」3 秒。
- **frontmatter 容错不静默**：YAML 非法或文件不可读 → `Frontmatter::Failed`，落到列表项可见文案「（frontmatter 解析失败）」，name 退回目录名/文件名；坏的单项不拖垮整表。

## 视图布局细节（v2.5.0）

**外层纸片包裹**：
```vue
<div class="h-full p-2.5">
  <div class="h-full flex bg-card border border-border rounded-lg shadow-paper overflow-hidden">
    <WorkshopNav />
    <main class="flex-1 min-w-0 overflow-y-auto px-6.5 py-5">
      <div class="content-area">
        <!-- 页头 + 列表 -->
      </div>
    </main>
  </div>
</div>
```

- 外层 `p-2.5` 留白（10px gap）
- 纸片 `bg-card border rounded-lg shadow-paper` + `overflow-hidden` 裁切圆角
- 右区 `overflow-y-auto` 内容滚动，`px-6.5 py-5` 内边距
- `content-area` 限宽（`max-w-280 mx-auto w-full`，资产列表在其内二次布局

**AssetList 双列网格（v2.5.0）**：
```vue
<div class="asset-grid">
  <AssetItem ... />
</div>
```
```css
.asset-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}
@media (max-width: 768px) {
  .asset-grid {
    grid-template-columns: 1fr;
  }
}
```

- 全宽双列（缩放 <768px 降单列）
- 项目间距 8px
- 每个 AssetItem 独立卡片（.fitem 样式，card 底 + border + shadow-paper + padding）

**WorkshopNav background 色（v2.5.0）**：
```css
.ws-nav {
  width: 190px;
  padding: 12px 8px;
  background: var(--background);  /* 新增：与 AutomationView 侧栏一致 */
}
```

## 关联条目

- [应用外壳](app-shell.md) — ActivityBar 域点亮 + TitleBar 工具按钮；workshop 是第四个点亮域
- [首页仪表板](home-dashboard.md) — 同构的「惰性加载单例 + spawn_blocking 扫描」域
- [项目发现](project-discovery.md) — 复用 `discovery::discover_all()` 拿项目 cwd
- [Tauri 桥梁](../shared/tauri-bridge.md) — 三个新 command 接口
- [MCP 协议](../integrations/mcp-protocol.md) — MCP 配置语义参考
- [只读陈列决策](../decisions/readonly-config-domains.md) — 为什么不做启停/编辑
- [MCP 探活 TLS 坑](../pitfalls/mcp-probe-tls-backend.md) — reqwest 默认无 TLS 后端
- [symlink 扫描坑](../pitfalls/skills-symlink-scan.md) — 软链 skill 目录跟随
- [样式系统 content-area](../shared/style-system.md) — content-area shortcut 的作用
```

---

## 5. docs/knowledge/domains/home-dashboard.md

```markdown
# 首页仪表板

v2.2.0 初版三卡（Token/诊断/热力）升级为 11 张数据面板，深度挖掘使用模式与工作节奏。DiagnosisCard 迁移至设置页「兼容性诊断」section。v2.5.0 整体纸片包裹，内容用 `content-area` 居中限宽。

## 架构

```
HomeView.vue (h-full overflow-y-auto px-8 py-6.5)
  ├── watch(activeSection === 'home') → ensureLoaded() + loadProjects()
  ├── headDate computed（依赖 refreshing 防跨午夜冻结）
  └── div.content-area
        ├─ flex.items-baseline.gap-3.mb-4.5 (headDate 说明文)
        └─ card-grid (11 张卡片，双列响应式)
              ├─ TodaySummaryCard    ← usage.daily + projects (今日概要)
              ├─ StreakCard           ← usage.daily (连续活跃)
              ├─ TokenCard            ← usage.month + daily (消耗+趋势)
              ├─ CostEstimateCard     ← projects (费用估算)
              ├─ RecentSessionsCard   ← projects (最近8会话, wide)
              ├─ ModelPreferenceCard  ← projects (模型偏好)
              ├─ ProjectActivityCard  ← projects (项目活跃)
              ├─ BranchActivityCard   ← projects (分支活跃)
              ├─ WorkRhythmCard       ← projects (作息节奏)
              ├─ SessionDepthCard     ← projects (会话深度)
              └─ HeatmapCard          ← usage.daily (热力图, wide, clickable)

数据源：
  useHomeStats (usage/diag) — Rust 端全量聚合
  useProjects (projects)   — 项目+会话摘要
```

## 11 张卡片摘要

| 卡片 | 数据源 | 核心指标 | 交互 |
|---|---|---|---|
| TodaySummaryCard | usage.daily + projects | 今日会话数/Token/模型 | 展示 |
| StreakCard | usage.daily | 当前连续天/最长记录/活跃天数 | 展示 |
| TokenCard | usage.month + daily | 本月总量 + top-5 模型分布 + 环比趋势 | retry |
| CostEstimateCard | projects | 按官方定价粗估月费(input/output/cache分计) | 展示 |
| RecentSessionsCard | projects (wide) | 最近 8 条会话(标题+项目+时间) | 点击跳转会话 |
| ModelPreferenceCard | projects | 按会话数统计模型使用分布 | 展示 |
| ProjectActivityCard | projects | top-6 活跃项目(会话数+最近活跃) | 展示 |
| BranchActivityCard | projects | top-8 活跃分支(会话数+最近活跃) | 展示 |
| WorkRhythmCard | projects | 24h 柱状图 + 高产时段 | hover |
| SessionDepthCard | projects | 消息数 4 档分桶(<10/10-50/50-200/200+) | hover |
| HeatmapCard | usage.daily (wide) | 16 周热力图，nearest-rank 五档 | 点击跳转会话列表 |

## 关键变更（vs v2.2.0）

- **DiagnosisCard 迁设置页**：开发者自检工具不属于用户仪表板，移到 SettingsView 新增「兼容性诊断」section
- **TokenCard 加趋势**：从 daily 数据聚合上月总量，计算环比百分比（↑/↓）
- **HeatmapCard 可点击**：有数据的格子显示 hover outline，点击跳转会话列表
- **数据源扩展**：新增 useProjects（projects 数据），HomeView 进入时同时加载 usage + projects
- **v2.5.0 纸片和 content-area**：无外层纸片（HomeView 本身即单主体），内容用 `content-area` 居中限宽、卡片双列响应式

## 布局细节（v2.5.0）

```vue
<template>
  <main class="h-full overflow-y-auto px-8 py-6.5">
    <div class="content-area">
      <div class="flex items-baseline gap-3 mb-4.5">
        <span class="text-xs text-muted-foreground">{{ headDate }}</span>
      </div>

      <div class="card-grid">
        <!-- 11 张卡片 -->
      </div>
    </div>
  </main>
</template>

<style scoped>
.card-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
@media (max-width: 768px) {
  .card-grid {
    grid-template-columns: 1fr;
  }
}
</style>
```

- 外层 main：`h-full overflow-y-auto px-8 py-6.5`（滚动容器、左右边距）
- `content-area` shortcut：`max-w-280 mx-auto w-full`（280px 限宽、水平居中、保持全宽响应）
- `card-grid`：双列网格 14px 间距，<768px 降单列
- 每张卡片：HomeCard.vue 包裹（card 底 + border + shadow-paper + hover 拈起）

## 关键文件

| 文件 | 职责 |
|---|---|
| `src/views/HomeView.vue` | 11 卡片容器、`content-area` 布局、数据引流、刷新编排 |
| `src/composables/useHomeStats.ts` | usage + diag 数据源单例 |
| `src/components/home/HomeCard.vue` | 卡片外壳基类（Paper 主题、hover 拈起） |
| `src/components/home/*.vue` | 11 张卡片组件 |
| `src/views/SettingsView.vue` | DiagnosisCard 迁入处（「诊断」section） |

## 关联条目

- Probe 模块：[`../shared/probe-module.md`](../shared/probe-module.md)
- Token 去重：[`../pitfalls/usage-dedup-message-id.md`](../pitfalls/usage-dedup-message-id.md)
- 分位数算法：[`../pitfalls/heatmap-quantile-nearest-rank.md`](../pitfalls/heatmap-quantile-nearest-rank.md)
- Computed 冻结：[`../pitfalls/computed-frozen-no-dep.md`](../pitfalls/computed-frozen-no-dep.md)
- 应用壳：[`./app-shell.md`](./app-shell.md)
- 样式系统 content-area：[`../shared/style-system.md`](../shared/style-system.md)
```

---

## 6. docs/knowledge/shared/style-system.md

**需要在 "UnoCSS 配置" 章节新增内容**：

```markdown
# 样式系统

UnoCSS preset-wind4 + preset-icons + Paper 主题 token（shadcn 语义命名，亮暗双套）。组件只消费 CSS 变量，零颜色字面量。

## 入口

`src/main.ts:2-5` 串起四层：
```
'virtual:uno.css'               # UnoCSS 注入
'./styles/paper/paper.css'      # Paper token 真理源（style-lab 复制快照）
'./styles/paper/extends.css'    # 项目扩展 token + body 基础样式
'./prose.css'                    # markdown 排版
```

UnoCSS 由 `vite.config.ts:9` 的 `UnoCSS()` 插件接入。

## Paper token 体系

- **paper.css = 上游真理源**：从 style-lab 整文件夹复制进 `src/styles/paper/`（README 使用规范随行），更新 = 重新复制覆盖，本仓不改其本体
- **extends.css = 项目扩展层**（独立文件，不随覆盖丢失）：`--claude` 陶土橙亮暗双值（Claude 身份色）、`--font-mono` 系统等宽栈、body 基础样式（13px / `var(--font-sans)` / 纸底——全局只在此定义一次，`extends.css:22-35`）
- **氛围层**：`paper-atmosphere`（颗粒 + vignette）挂 body（`index.html:8`），**全站仅此一处，子容器禁止重复叠加**。z-index 口径：paper README 写「颗粒盖在内容之上」，P1' 冻结原型则把内容容器提到氛围层之上（`.app{z-index:1}`），实施拍板按原型口径——但当前 App 根（`App.vue:37`）未显式定位，氛围层伪元素（`z-index:0`）实际仍绘于静态内容之上；7% multiply 下两口径肉眼无差，给壳根加定位/z-index 时需留意

## UnoCSS 配置

`uno.config.ts` 关键点：

- **theme.colors**（`uno.config.ts:41-87`）整体为 shadcn 语义族：background / foreground / card / popover / primary / secondary / muted / accent / destructive / border / input / ring + 扩展 `claude`，值全是 `var()` **零字面量**；`font.sans/mono`、`radius.DEFAULT` 同对齐
- **rules**：`shadow-paper` / `shadow-paper-lifted` 两枚（`uno.config.ts:36-40`）——**铁律：阴影只用这两个 token**（中性灰阴影瞬间破坏纸感）
- **shortcuts**：
  - style-lab 继承收缩为 `center` / `flex-center` 两个中性项（`uno.config.ts:10-13` filter），glass 体系全部不挂
  - **v2.5.0 新增 `content-area`**（`uno.config.ts:37`）：`'max-w-280 mx-auto w-full'` — 限宽 280px（shadcn Tailwind 等效）、水平自动居中、保持全宽响应。**作用**：统一多个域（Home / Workshop / Automation / Settings）的内容区宽度与居中策略，替代逐组件 `max-w-xs` + `mx-auto` 手工管理。实施要点：HomeView 卡片容器 / WorkshopView 右区列表 / AutomationView 内容区都包装在 `content-area` div 内
- **dark: 'class'** — 暗色由 `<html class="dark">` 切换；**preset-icons** carbon 集合；transformers / safelist 同前
- 注意：preset-wind4 的颜色产出形式是 `color-mix(in srgb, var(--card) var(--un-bg-opacity), transparent)` 而非朴素 `var()`——grep 编译产物或写断言时按此匹配

## 桌面质感约束（Paper 铁律）

- 禁 backdrop-blur / 霓虹 / 大渐变（Glass 专属，已随换肤下线）；无纯黑纯白；零 webfont
- 圆角 ≤ 6px（`--radius: 0.3rem`）；紧凑间距（4-8px）；Carbon 单色描边图标
- **三档深度隐喻**：ActivityBar = `secondary`（桌沿）/ 侧栏列表 = `background`（桌面）/ 详情 pane = `card`（浮起的纸）
- hover「拈起」模式：`shadow-paper` → `shadow-paper-lifted` + 上移 1px（示例 `HomeView.vue:139-142`）

## 动画规范

- 分屏 pane：`flex 200ms cubic-bezier(0.32, 0.72, 0, 1)`（`SplitView.vue:79-81`），拖拽期间 `.no-transition` 关闭
- 侧栏折叠：`width 220ms` 同曲线（`SessionsView.vue:48-50`）

## prose-msg

`src/prose.css` 已对齐 Paper token：行内代码 = 衬纸方签（`--background` + `--border`，`prose.css:39-46`），代码块 `--radius` 圆角 + `--font-mono`，表头 `--secondary`，链接 `--primary` 常显下划线。shiki 双主题切换机制不变（`.dark .prose-msg pre.shiki` 选择器）。

## 关联条目

- [UnoCSS + style-lab 集成](../integrations/unocss-style-lab.md) — 收缩后的依赖关系与快照语义
- [Paper 替换 Glass 决策](../decisions/paper-over-glass.md) — 铁律与语义重映射的来由
- [主题切换](../domains/theming.md) — 暗色零特判与 `.dark` token 块
- [分屏布局](../domains/split-panes.md) — 浮起 pane 与 transition 规范
- [应用壳](../domains/app-shell.md) — 三档深度隐喻的落点
```

---

## 7. docs/knowledge/workflows/add-activitybar-domain.md

```markdown
# 工作流：新增一个 ActivityBar 域

新增一个 ActivityBar 顶部域（如工坊 / 自动化 / 未来的搜索）的端到端改动清单，并规避注册表热点的并行冲突。

## 步骤

### 1. Rust 新建独立模块

`src-tauri/src/<domain>.rs`，自带 `#[tauri::command]`——**不挤进 commands.rs**，仿 `workshop.rs` / `automation.rs` / `channels.rs`。

### 2. lib.rs 注册

`src-tauri/src/lib.rs`：加 `mod <domain>`，并在 `generate_handler!` 列表注册 `<domain>::xxx`。

### 3. composable

`src/composables/use<Domain>.ts`：模块级单例 + 惰性加载，仿 `useWorkshop` / `useAutomation`。

### 4. 视图

`src/views/<Domain>View.vue`。

### 5. AppSection 联合 + 白名单

`src/composables/useUiState.ts`：`AppSection` 联合类型加 `'<domain>'`，并加入 `validSections` 白名单（非法值回退 `sessions`）。

### 6. ActivityBar 点亮

`src/components/ActivityBar.vue`：`topDomains` 加一项 `{ section: '<domain>', ... }` 点亮入口。

### 7. TitleBarTools 条件分支（v2.5.0）

`src/components/TitleBarTools.vue`：若域有工具按钮（搜索、刷新、打开配置等），加 `v-if="activeSection === '<domain>'"` 分支渲染相应工具按钮。

### 8. App.vue 接入

`src/App.vue`：
- `<Domain>View` 导入（line 1-13）
- `<Domain>View` 用 `v-show` 接入（DOM 常驻、切域不卸载，line 77 前后）
- 若 TitleBar 需特殊内容（如 WorkbenchTabs），加 `#leading` slot 分支

## 注意事项

并行开发纪律——7 处注册表热点（`lib.rs` / `useUiState.ts` / `ActivityBar.vue` / `TitleBarTools.vue` / `App.vue` / `views/<Domain>View.vue` / `composables/use<Domain>.ts`）多分支并行会连环冲突。约定：

- 各域**新建独立模块文件**，不挤 commands.rs，从源头减少热点。
- 域"点亮"改动（步骤 5-8）放分支**最后一个 commit**；先合的分支先 rebase，冲突全是机械"取并集"。
- 共享文件（如 `discovery.rs`）**写权单一**，其他域只调用已 `pub` 的函数，不改其内部。
- `v-show` 接入意味组件 DOM 常驻——隐藏域的全局监听器仍活跃，注意清理（见 [hidden-component-global-listeners](../pitfalls/hidden-component-global-listeners.md)）。
- **TitleBar 工具按钮设计**（v2.5.0）：按 activeSection 条件分支，同域多个工具按钮用 `<template v-if="...">` 包组，toolbar-specific logic（搜索输入、刷新状态）仅在该域显示；按钮样式统一为 `.tb-btn` class（card 底 + border + hover shadow-paper）。

## 关联条目

- [应用壳：TitleBar + ActivityBar + 域切换](../domains/app-shell.md) — 域切换与 ActivityBar 全景、TitleBar slot 机制
- [工坊域](../domains/workshop.md) — 参照实现之一
- [自动化域](../domains/automation.md) — 参照实现之一（侧栏导航 + content-area）
- [工作流：新增 Tauri Command](./add-tauri-command.md) — 单个 command 的细粒度步骤
```

---

## 需要新增的条目

### 新增文件：docs/knowledge/domains/workbench-tabs.md

```markdown
# 工作台 Tab 条（TitleBar 内）

v2.5.0 WorkbenchTabs 从 WorkbenchView 顶部移到 TitleBar 的 `#leading` slot 中渲染，成为全宽标题栏的左侧主要内容（ActivityBar 下方）。视觉上形成上部连续的 tab 管理条，与 ActivityBar 纵向对齐。

## 位置与布局

```
App.vue
  <TitleBar>
    <template #leading>
      <WorkbenchTabs v-if="activeSection === 'workbench'" />
    </template>
  </TitleBar>
```

- WorkbenchTabs 仅在 `activeSection === 'workbench'` 时显示
- 其他域显示域名文字（档案/工坊/自动化/首页/设置）
- 关键：TitleBar `#leading` slot 用 `min-w-0` 防缩写，WorkbenchTabs 自身用 `overflow-x-auto` 处理 tab 溢出

## 样式与功能（v2.5.0 无变更）

相对 v2.4.0，v2.5.0 WorkbenchTabs 本身逻辑和样式完全不变——只是从 `WorkbenchView.vue:9-14` 的顶部降级移位到 TitleBar 的 slot。关键特性：

- **Tab 管理**：创建、重命名（双击 / 右键菜单）、关闭（需确认 > 1 个）、拖拽重排（drop indicator = primary 竖线）、加号新建
- **跨域拖拽接点**（FR-005）：接收 SessionList 卡片拖入，关联卡片到该工作台（sessionHoverTabId 高亮）
- **横向滚动**：tab 数量过多时 `overflow-x-auto` + `tabs-scroll`（scrollbar-width: thin）
- **最后弹性区挂 data-tauri-drag-region**（`WorkbenchTabs.vue:192`）：与 TitleBar + ActivityBar 形成连续拖拽条

## 关键改动（v2.5.0）

- `src/App.vue:64-70`：TitleBar `#leading` slot 加 WorkbenchTabs 条件渲染
- 样式保持 `src/components/workbench/WorkbenchTabs.vue:145-248` 原状（无改）
- 交互逻辑保持原状（tab 拖拽、会话拖入等）

## 关联条目

- [应用壳](./app-shell.md) — TitleBar 架构与 slot 机制
- [工作台](./workbench.md) — WorkbenchTabs 的工作台域背景
```

---

## 总体更新总结

**受影响的 7 个知识库条目**（按更新量排序）：

1. **app-shell.md** — 最关键，添加 TitleBar 整节、更新 ActivityBar 布局、更新红绿灯方案
2. **automation.md** — 添加 v2.5.0 侧栏导航纸片包裹、content-area 布局描述
3. **workshop.md** — 添加 v2.5.0 纸片包裹、background 色、content-area、双列 grid
4. **architecture-overview.md** — 更新组件树引用（TitleBar / TitleBarTools）、command 列表添加 workshop / automation
5. **home-dashboard.md** — 补充 content-area 布局细节、没有纸片包裹（HomeView 本身即主体）
6. **style-system.md** — 新增 content-area shortcut 的用途说明与实施要点
7. **add-activitybar-domain.md** — 新增步骤 7（TitleBarTools）、更新注意事项

**新增 1 个条目**：
- **workbench-tabs.md** — 独立记录 v2.5.0 WorkbenchTabs 位置迁移及 TitleBar slot 机制