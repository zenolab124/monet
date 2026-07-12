# Monet

[Claude Code](https://docs.anthropic.com/en/docs/claude-code) 的任务控制中心 —— 浏览、搜索、管理会话的桌面应用。

基于 [Tauri 2](https://tauri.app/) + [Vue 3](https://vuejs.org/) 构建。

> **[English](README.md)**

<!-- TODO: 添加截图 -->
<!-- ![Monet 截图](docs/screenshot.png) -->

## 功能

- **会话浏览** — 解析 `~/.claude/projects/` 下所有 JSONL 会话记录，支持排序、筛选、全文搜索
- **丰富的对话视图** — Markdown 渲染、代码语法高亮 (Shiki)、工具调用展开、思考过程显示
- **流式交互** — 发送跟进消息，实时流式渲染，Esc 中断
- **工作台** — 多标签页会话工作区，支持拖拽分屏
- **档案馆** — 三栏只读会话查阅（项目 → 会话列表 → 详情）
- **实时刷新** — 后台文件监控，会话变化时自动更新
- **会话操作** — 终端恢复 (`claude --resume`)、VS Code 打开、删除
- **多语言** — 内置 12 种语言，支持 AI 翻译扩展任意语言
- **外观** — 暗色 / 亮色 / 跟随系统，macOS 原生标题栏

## 安装

从 [Releases](../../releases) 下载最新 `.dmg`。

> 目前仅支持 macOS，需要 macOS 11+。

## 从源码构建

### 前置条件

- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 10+
- [Rust](https://rustup.rs/) 1.77+
- Xcode Command Line Tools — `xcode-select --install`

### 构建

```bash
git clone https://github.com/zenolab124/monet.git
cd monet
pnpm install
pnpm tauri build
```

产物在 `src-tauri/target/release/bundle/macos/` 下。

开发模式：

```bash
pnpm tauri dev
```

## 工作原理

Monet 读取 `~/.claude/projects/` 下的 Claude Code 会话数据，**绝不写入** JSONL 文件 — 所有增值数据（标题、标签、归档状态）存储在独立目录 `~/.monet/`。

## 技术栈

- [Tauri 2](https://tauri.app/) — Rust 后端 + 系统 WebView
- [Vue 3](https://vuejs.org/) + TypeScript + Composition API
- [UnoCSS](https://unocss.dev/) — 原子化 CSS (preset-wind4 + preset-icons)
- [Shiki](https://shiki.style/) — 语法高亮
- [markdown-it](https://github.com/markdown-it/markdown-it) — Markdown 渲染
- [vue-i18n](https://vue-i18n.intlify.dev/) — 国际化
- [@dnd-kit/vue](https://dndkit.com/) — 拖拽

## 开源协议

[MIT](LICENSE)
