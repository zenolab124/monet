<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Monet">
</p>

<h1 align="center">Monet</h1>

<p align="center">
  <a href="https://docs.anthropic.com/en/docs/claude-code">Claude Code</a> 的任务控制中心
</p>

<p align="center">
  <a href="README.md">English</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/zenolab124/monet" alt="License"></a>
  <a href="https://github.com/zenolab124/monet/releases/latest"><img src="https://img.shields.io/github/v/release/zenolab124/monet" alt="Latest Release"></a>
  <a href="https://github.com/zenolab124/monet/actions/workflows/ci.yml"><img src="https://github.com/zenolab124/monet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS-purple" alt="platform">
  <img src="https://img.shields.io/badge/built_with-Tauri_2-24c8db" alt="Tauri">
  <img src="https://img.shields.io/badge/Vue-3.5-42b883" alt="Vue">
  <img src="https://img.shields.io/badge/Rust-stable-e57324" alt="Rust">
  <img src="https://img.shields.io/badge/TypeScript-6-3178c6" alt="TypeScript">
</p>

<p align="center">
  <a href="#功能">功能</a> •
  <a href="#安装">安装</a> •
  <a href="#从源码构建">构建</a> •
  <a href="#数据与隐私">隐私</a>
</p>

<p align="center">
  <img src=".github/assets/workbench-hero.webp" alt="Monet 工作台——多列会话、会话监控、内嵌 HTML 渲染与实时后台任务" width="920">
</p>

## Monet 是什么？

一个桌面应用，把 Claude Code 的会话历史变成可浏览、可搜索、可交互的工作空间。名字取自 [Claude Monet](https://zh.wikipedia.org/wiki/%E5%85%8B%E5%8A%B3%E5%BE%B7%C2%B7%E8%8E%AB%E5%A5%88)——对，就是那个 Claude。

Monet **绝不写入** Claude Code 的 JSONL 文件，所有增值数据存储在独立目录 `~/.monet/`。

## 功能

**回顾** — 跨项目浏览所有会话，毫秒级全文搜索，Markdown / 代码 / 工具调用渲染，思考块展开。

**推进** — 多标签工作台，可拖拽分列。发送跟进消息实时流式渲染。随时切换模型、渠道、思考力度。

**调度** — 用 cron 表达式定时运行 Claude Code 任务。支持 macOS 睡眠唤醒，适合过夜跑任务。

**集成** — 内置 MCP server（`monet-mcp`），让 Claude Code 在 CLI 中搜索会话历史、管理定时任务。macOS 小组件随时查看统计。

**个性化** — 内置 12 种语言 + AI 翻译扩展任意语言。暗色 / 亮色 / 跟随系统。macOS 原生标题栏。系统权限体检面板。

## 安装

**Homebrew**：

```bash
brew tap zenolab124/tap
brew install --cask monet
```

或从 [Releases](../../releases) 下载最新 `.dmg`。

> 目前仅支持 macOS，需要 macOS 11+（Apple Silicon）。

**首次打开**：Monet 使用稳定签名身份但尚未经过 Apple 公证，首次打开会触发 Gatekeeper 提示。右键应用 → **打开**（仅需一次），或执行：

```bash
xattr -cr /Applications/Monet.app
```

之后的版本更新由应用内静默完成，不再有任何提示。

## 从源码构建

### 前置条件

- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 10+
- [Rust](https://rustup.rs/) 1.77+
- Xcode Command Line Tools — `xcode-select --install`

### 开发模式

```bash
git clone https://github.com/zenolab124/monet.git
cd monet
pnpm install
pnpm tauri dev
```

### 发布构建（含小组件 + 签名）

```bash
pnpm release
```

依次执行 `tauri build`、编译 macOS 小组件、嵌入 app bundle、签名、生成 `.dmg`。

建立本机签名身份（推荐——TCC 权限跨构建保持稳定）：

```bash
scripts/setup-signing.sh
```

不跑也能构建，会降级为 ad-hoc 签名——功能正常，但每次重新构建后 TCC 权限需重新授予，小组件可能不注册。

## 数据与隐私

| 内容 | 位置 | 访问方式 |
|------|------|---------|
| Claude Code 会话 | `~/.claude/projects/` | **只读** |
| Monet 增值数据（标题、标签、定时任务） | `~/.monet/` | 读写 |
| MCP 注册 | `~/.claude/settings.json` | 在 `mcpServers` 下添加 `monet` 条目 |

Monet 完全离线运行。无遥测、无账号、无网络请求（除非你主动通过 Claude Code CLI 发送流式消息）。

## 技术栈

- [Tauri 2](https://tauri.app/) — Rust 后端 + 系统 WebView
- [Vue 3](https://vuejs.org/) + TypeScript + Composition API
- [UnoCSS](https://unocss.dev/) — 原子化 CSS (preset-wind4 + preset-icons)
- [Shiki](https://shiki.style/) — 语法高亮
- [markdown-it](https://github.com/markdown-it/markdown-it) — Markdown 渲染
- [vue-i18n](https://vue-i18n.intlify.dev/) — 国际化
- [@dnd-kit/vue](https://dndkit.com/) — 拖拽
- [Swift WidgetKit](https://developer.apple.com/documentation/widgetkit) — macOS 小组件

## 开源协议

[MIT](LICENSE)
