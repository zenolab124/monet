# CC Space

跨平台 Claude Code 会话管理器，用 Tauri 2 + Vue 3 构建的桌面应用。

## 功能

- **三栏布局**：项目 / 会话 / 详情
- **会话浏览**：解析 `~/.claude/projects/` 下的 JSONL 记录，支持排序、筛选、全文搜索
- **对话详情**：Markdown 渲染、代码高亮（Shiki）、工具调用展开、私有标签识别
- **流式响应**：输入框发送跟进消息、实时流式渲染、Esc 中断
- **分屏系统**：递归分屏（水平/垂直）、拖拽调整比例、布局持久化
- **会话操作**：终端恢复（`claude --resume`）、VSCode 打开、删除
- **文件监控**：后台监听 JSONL 变化，自动刷新列表
- **外观**：暗色/亮色/跟随系统，macOS 毛玻璃背景

## 技术栈

- [Tauri 2](https://tauri.app/) — Rust 后端 + WebView 前端
- [Vue 3](https://vuejs.org/) + TypeScript + Composition API
- [UnoCSS](https://unocss.dev/) — 原子化 CSS，preset-wind4 + preset-icons
- [Shiki](https://shiki.style/) — 语法高亮

## 开发

```bash
pnpm install
pnpm tauri dev       # 启动开发模式（前后端）
pnpm tauri build     # 打包发布
```

## 目录结构

```
cc-space-tauri/
├── src/                  # Vue 3 前端
│   ├── components/       # UI 组件
│   ├── composables/      # 状态/逻辑（useProjects / useSessions / useSplitLayout ...）
│   ├── views/
│   └── types/
├── src-tauri/            # Rust 后端
│   └── src/
│       ├── models/       # 数据模型
│       ├── parser.rs     # JSONL 解析
│       ├── discovery.rs  # 项目发现
│       ├── streaming.rs  # Claude CLI 流式进程
│       └── watcher.rs    # 文件监控
└── docs/knowledge/       # 项目知识库
```

更多架构细节见 [docs/knowledge/INDEX.md](docs/knowledge/INDEX.md) 和 [CLAUDE.md](CLAUDE.md)。

## 数据路径

- 会话：`~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`
- 编码规则：目录名中 `-` 对应路径分隔符 `/`，首个 `-` 为根

## License

MIT
