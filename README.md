<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Monet">
</p>

<h1 align="center">Monet</h1>

<p align="center">
  Mission Control for <a href="https://docs.anthropic.com/en/docs/claude-code">Claude Code</a>
</p>

<p align="center">
  <a href="README.zh-CN.md">中文说明</a>
</p>

<!-- TODO: screenshot -->

## What is Monet?

A desktop app that turns your Claude Code session history into a browsable, searchable, and interactive workspace. Named after [Claude Monet](https://en.wikipedia.org/wiki/Claude_Monet) — yes, that Claude.

Monet **never writes** to Claude Code's JSONL files. All metadata is stored separately in `~/.monet/`.

## Features

**Review** — Browse all sessions across projects, full-text search in milliseconds, rich Markdown/code/tool-call rendering, thinking block expansion.

**Work** — Multi-tab workbench with draggable split columns. Send follow-up messages with real-time streaming. Switch models, channels, and effort levels on the fly.

**Automate** — Schedule recurring Claude Code tasks with cron expressions. macOS wake-from-sleep support for overnight runs.

**Integrate** — Built-in MCP server (`monet-mcp`) lets Claude Code search your session history and manage routines from the CLI. macOS widgets for at-a-glance stats.

**Customize** — 12 built-in languages + AI translation for any language. Dark / light / system theme. macOS native title bar. System permission health check panel.

## Install

Download the latest `.dmg` from [Releases](../../releases).

> macOS only for now. Requires macOS 11+.

**First launch**: Monet is signed with a stable identity but not yet notarized by Apple, so Gatekeeper will warn on first open. Right-click the app → **Open** (once), or run:

```bash
xattr -cr /Applications/Monet.app
```

After that, updates install silently in-app — no warnings again.

## Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 10+
- [Rust](https://rustup.rs/) 1.77+
- Xcode Command Line Tools — `xcode-select --install`

### Development

```bash
git clone https://github.com/zenolab124/monet.git
cd monet
pnpm install
pnpm tauri dev
```

### Release Build (with widget + signing)

```bash
pnpm release
```

This runs `tauri build`, compiles the macOS widget extension, embeds it into the app bundle, signs everything, and creates a `.dmg`.

To set up a local signing identity (recommended — keeps TCC permissions stable across rebuilds):

```bash
scripts/setup-signing.sh
```

Without it, the build falls back to ad-hoc signing — functional, but TCC permissions reset on each rebuild and widgets may not register.

## Data & Privacy

| What | Where | Access |
|------|-------|--------|
| Claude Code sessions | `~/.claude/projects/` | **Read-only** |
| Monet metadata (titles, tags, routines) | `~/.monet/` | Read-write |
| MCP registration | `~/.claude/settings.json` | Adds `monet` key |

Monet is fully offline. No telemetry, no accounts, no network calls (except when you explicitly use streaming via Claude Code CLI).

## Tech Stack

- [Tauri 2](https://tauri.app/) — Rust backend + system WebView
- [Vue 3](https://vuejs.org/) + TypeScript + Composition API
- [UnoCSS](https://unocss.dev/) — Atomic CSS (preset-wind4 + preset-icons)
- [Shiki](https://shiki.style/) — Syntax highlighting
- [markdown-it](https://github.com/markdown-it/markdown-it) — Markdown rendering
- [vue-i18n](https://vue-i18n.intlify.dev/) — i18n
- [@dnd-kit/vue](https://dndkit.com/) — Drag and drop
- [Swift WidgetKit](https://developer.apple.com/documentation/widgetkit) — macOS widgets

## License

[MIT](LICENSE)
