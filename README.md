# CC Space

A desktop app for browsing, searching, and managing [Claude Code](https://docs.anthropic.com/en/docs/claude-code) sessions.

Built with [Tauri 2](https://tauri.app/) + [Vue 3](https://vuejs.org/).

> **[中文说明](README.zh-CN.md)**

<!-- TODO: add screenshot here -->
<!-- ![CC Space screenshot](docs/screenshot.png) -->

## Features

- **Session browser** — Parses all JSONL records from `~/.claude/projects/`, with sorting, filtering, and full-text search
- **Rich conversation view** — Markdown rendering, syntax highlighting (Shiki), tool call expansion, thinking block display
- **Streaming** — Send follow-up messages with real-time streaming, press Esc to interrupt
- **Workbench** — Tabbed multi-session workspace with draggable split panes
- **Archive** — Three-panel read-only session explorer (projects → sessions → detail)
- **Live refresh** — Background file watcher auto-updates when sessions change
- **Session actions** — Resume in terminal (`claude --resume`), open in VS Code, delete
- **i18n** — 12 built-in languages, plus AI-powered translation for any language
- **Appearance** — Dark / light / system theme, macOS native title bar

## Install

Download the latest `.dmg` from [Releases](../../releases).

> macOS only for now. Requires macOS 11+.

## Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 10+
- [Rust](https://rustup.rs/) 1.77+
- Xcode Command Line Tools — `xcode-select --install`

### Build

```bash
git clone https://github.com/zenolab124/cc-space.git
cd cc-space
pnpm install
pnpm tauri build
```

The `.dmg` and `.app` will be in `src-tauri/target/release/bundle/macos/`.

For development:

```bash
pnpm tauri dev
```

## How It Works

CC Space reads Claude Code session data from `~/.claude/projects/`. It **never writes** to these JSONL files — all metadata (titles, tags, archive status) is stored separately in `~/.claude/cc-space/`.

## Tech Stack

- [Tauri 2](https://tauri.app/) — Rust backend + system WebView
- [Vue 3](https://vuejs.org/) + TypeScript + Composition API
- [UnoCSS](https://unocss.dev/) — Atomic CSS (preset-wind4 + preset-icons)
- [Shiki](https://shiki.style/) — Syntax highlighting
- [markdown-it](https://github.com/markdown-it/markdown-it) — Markdown rendering
- [vue-i18n](https://vue-i18n.intlify.dev/) — Internationalization
- [@dnd-kit/vue](https://dndkit.com/) — Drag and drop

## License

[MIT](LICENSE)
