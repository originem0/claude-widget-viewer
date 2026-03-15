[English](README.md) | [中文](README.zh-CN.md)

# claude-widget-viewer

Lightweight native widget renderer that brings [claude.ai's generative UI](https://claude.com/blog/claude-builds-visuals) to Claude Code. Rust + WebView2, single 756KB binary.

Claude Code can't render HTML in the terminal. This tool bridges that gap: Claude writes widget HTML to a file, a hook detects it, and a native WebView2 window pops up with the rendered result — charts, diagrams, interactive controls, all running in a real browser engine.

```
Claude Code ─── Write ──→ .claude/widgets/chart.html
                                    │
                          PostToolUse hook
                                    │
                          claude-widget-viewer.exe send chart.html
                                    │
                              Named Pipe IPC
                                    │
                          daemon (prewarmed WebView2)
                                    │
                          Native window renders widget
```

## Prerequisites

- **Windows 11** (WebView2 ships pre-installed)
- **Rust 1.77+** with MSVC toolchain
- **MSVC Build Tools** + **Windows SDK** (for compiling wry/WebView2 bindings)
- **jq** (for hook scripts to parse JSON)
- **Claude Code** CLI

### Installing prerequisites (scoop)

```bash
scoop install rust jq
winget install Microsoft.VisualStudio.2022.BuildTools --silent --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --includeRecommended"
```

### Cargo mirror (China)

If `cargo build` hangs on "Updating crates.io index", add a mirror to `~/.cargo/config.toml`:

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
```

## Build

```bash
git clone git@github.com:originem0/claude-widget-viewer.git
cd claude-widget-viewer
cargo build --release
```

Binary at `target/release/claude-widget-viewer.exe` (~756KB).

## Install

### 1. Put binary on PATH

```bash
cp target/release/claude-widget-viewer.exe <somewhere-on-PATH>/
# e.g. scoop shims:
cp target/release/claude-widget-viewer.exe ~/scoop/shims/
```

### 2. Deploy hook scripts

```bash
mkdir -p ~/.claude/hooks
cp hook/widget-daemon-start.sh ~/.claude/hooks/
cp hook/post-write-widget.sh ~/.claude/hooks/
```

### 3. Configure Claude Code hooks

Add to `~/.claude/settings.json` (merge into existing `"hooks"` key):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [{
          "type": "command",
          "command": "bash ~/.claude/hooks/widget-daemon-start.sh"
        }]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Write",
        "hooks": [{
          "type": "command",
          "command": "bash ~/.claude/hooks/post-write-widget.sh"
        }]
      }
    ]
  }
}
```

### 4. Install the skill (optional)

Copy `claude/SKILL.md` to your skills directory so Claude auto-loads widget instructions when you ask for visualizations:

```bash
mkdir -p ~/.claude/skills/widget-viewer
cp claude/SKILL.md ~/.claude/skills/widget-viewer/SKILL.md
```

Without the skill, you can still add the widget protocol to your project's `CLAUDE.md` manually (see `claude/widget-protocol.md`).

## Usage

### Automatic (via Claude Code hooks)

Start a new Claude Code session. The daemon launches in the background. Ask Claude anything visual:

> "Draw a line chart of weekly temperatures"

Claude writes `.claude/widgets/temperature_chart.html` → hook triggers → window pops up with Chart.js rendering.

### Manual

```bash
# Standalone mode (opens window + watches file for changes)
claude-widget-viewer show path/to/widget.html

# Daemon mode (hidden window, prewarmed WebView2, listens on Named Pipe)
claude-widget-viewer listen

# Send widget to running daemon (falls back to show if no daemon)
claude-widget-viewer send path/to/widget.html

# Stop daemon
claude-widget-viewer stop
```

### Hot reload

Edit the widget HTML file while the viewer is running — changes appear instantly (200ms debounce). Useful for iterating on a widget's design.

## Widget HTML format

Widgets are raw HTML fragments. No `<!DOCTYPE>`, `<html>`, `<head>`, or `<body>` tags. Structure: style first, content second, script last.

```html
<style>
  .card {
    padding: var(--spacing-md);
    background: var(--color-bg-secondary);
    border-radius: var(--border-radius-lg);
  }
</style>

<div class="card">
  <h2>Weekly temperature</h2>
  <canvas id="chart"></canvas>
</div>

<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js"
        onload="initChart()"></script>
<script>
function initChart() {
  new Chart(document.getElementById('chart'), { /* ... */ });
}
if (window.Chart) initChart();
</script>
```

### CSS variables

The viewer injects a design system matching claude.ai's style. Use these variables:

| Category | Variables |
|----------|-----------|
| Text | `--color-text-primary`, `--color-text-secondary` |
| Background | `--color-bg-primary`, `--color-bg-secondary` |
| Border | `--color-border`, `--color-border-light` |
| Semantic | `--color-blue`, `--color-green`, `--color-amber`, `--color-red` |
| Category | `--color-purple`, `--color-teal`, `--color-coral`, `--color-pink` |
| Spacing | `--spacing-xs` (4px) through `--spacing-xl` (32px) |
| Radius | `--border-radius-sm` / `md` / `lg` |
| Font | `--font-sans`, `--font-mono` |

Light and dark mode switch automatically via `prefers-color-scheme`.

### CDN

Only `https://cdnjs.cloudflare.com` is allowed. Always use the `onload` + fallback pattern for CDN scripts.

## Architecture

```
src/
  main.rs       CLI entry, 4 subcommands (show/listen/send/stop)
  viewer.rs     winit window + wry WebView, event loop
  protocol.rs   wry:// custom protocol handler
  shell.rs      HTML shell generation, base64 injection
  watcher.rs    File watcher with 200ms debounce (notify crate)
  ipc.rs        Windows Named Pipe server/client (windows-sys)

assets/
  design-system.css   CSS variables (light/dark), SVG classes
  morphdom.min.js     DOM diffing library (12KB, inlined at compile time)
```

All assets are embedded via `include_str!` at compile time — zero runtime file dependencies.

### IPC protocol

The daemon listens on `\\.\pipe\claude-widget-viewer-{pid}`. Messages are JSON, one per connection:

```json
{"type":"LoadWidget","file":"C:/path/to/widget.html","title":"my_chart"}
{"type":"UpdateWidget","html":"<div>partial update</div>"}
{"type":"Show"}
{"type":"Close"}
```

The `send` subcommand handles all pipe communication. Hook scripts never touch Named Pipes directly.

## Limitations

- **Windows only** — uses WebView2 (WKWebView port would be needed for macOS)
- **No streaming** — Claude generates complete HTML before writing. `UpdateWidget` IPC message is reserved for future MCP streaming support
- **No sendPrompt()** — widgets can't send messages back to Claude yet (stub exists, needs MCP integration)
- **Single window** — new widgets replace the current one

## License

MIT
