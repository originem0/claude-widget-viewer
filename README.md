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

## Quick Install

**Requirements:** Windows 10/11 with [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on Windows 11, available for Windows 10). [jq](https://jqlang.github.io/jq/) for hook scripts.

**One-line install** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/originem0/claude-widget-viewer/main/install.ps1 | iex
```

Or clone and run locally:

```powershell
git clone https://github.com/originem0/claude-widget-viewer.git
cd claude-widget-viewer
powershell -ExecutionPolicy Bypass -File install.ps1
```

The installer downloads the pre-built binary, deploys hooks, configures `settings.json`, and installs the skill. No build tools needed.

**To uninstall:**

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -Uninstall
```

## Build from Source

Only needed if you want to modify the code or build for a different configuration.

**Build prerequisites:** Rust 1.77+ (MSVC toolchain), MSVC Build Tools, Windows SDK.

```bash
# Install build tools (if not present)
scoop install rust
winget install Microsoft.VisualStudio.2022.BuildTools --silent --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --includeRecommended"

# Build
git clone https://github.com/originem0/claude-widget-viewer.git
cd claude-widget-viewer
cargo build --release
# Binary: target/release/claude-widget-viewer.exe (~756KB)
```

<details>
<summary>Cargo mirror for China</summary>

If `cargo build` hangs on "Updating crates.io index", add to `~/.cargo/config.toml`:

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
```
</details>

After building, run `install.ps1` to deploy hooks and configure Claude Code, or do it manually:

<details>
<summary>Manual setup</summary>

1. Copy `target/release/claude-widget-viewer.exe` to somewhere on PATH
2. Copy `hook/*.sh` to `~/.claude/hooks/`
3. Merge hooks config into `~/.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [{ "type": "command", "command": "bash ~/.claude/hooks/widget-daemon-start.sh" }] }
    ],
    "PostToolUse": [
      { "matcher": "Write", "hooks": [{ "type": "command", "command": "bash ~/.claude/hooks/post-write-widget.sh" }] }
    ]
  }
}
```

4. (Optional) Copy `claude/SKILL.md` to `~/.claude/skills/widget-viewer/SKILL.md`
</details>

## Usage

### Automatic (via Claude Code hooks)

Start a new Claude Code session. The daemon launches in the background. Ask Claude anything visual:

> "Draw a line chart of weekly temperatures"

Claude writes `.claude/widgets/temperature_chart.html` → hook triggers → window pops up with Chart.js rendering.

### Manual

```bash
claude-widget-viewer show path/to/widget.html    # Open + watch for changes
claude-widget-viewer listen                       # Daemon mode (prewarmed)
claude-widget-viewer send path/to/widget.html     # Send to daemon
claude-widget-viewer stop                         # Stop daemon
```

### Hot reload

Edit the widget HTML file while the viewer is running — changes appear instantly (200ms debounce).

## Widget HTML Format

Widgets are raw HTML fragments. No `<!DOCTYPE>`, `<html>`, `<head>`, or `<body>`. Structure: style first, content second, script last.

```html
<style>
  .card { padding: var(--spacing-md); background: var(--color-bg-secondary); }
</style>
<div class="card">
  <canvas id="chart"></canvas>
</div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js" onload="initChart()"></script>
<script>
function initChart() { new Chart(document.getElementById('chart'), { /* ... */ }); }
if (window.Chart) initChart();
</script>
```

### CSS Variables

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

Light/dark mode switches automatically via `prefers-color-scheme`. Only `https://cdnjs.cloudflare.com` is allowed for CDN scripts.

## Architecture

```
src/
  main.rs       CLI entry, 4 subcommands (show/listen/send/stop)
  viewer.rs     winit window + wry WebView, event loop
  protocol.rs   wry:// custom protocol handler
  shell.rs      HTML shell generation, base64 injection
  watcher.rs    File watcher with 200ms debounce (notify crate)
  ipc.rs        Windows Named Pipe server/client (windows-sys)
```

All assets embedded at compile time via `include_str!` — zero runtime file dependencies. Daemon IPC via `\\.\pipe\claude-widget-viewer-{pid}` (JSON messages).

## Limitations

- **Windows only** — uses WebView2 (WKWebView port needed for macOS)
- **No streaming** — widget renders after Claude finishes writing (`UpdateWidget` IPC reserved for future MCP support)
- **No sendPrompt()** — widgets can't message Claude back yet (stub exists, needs MCP)
- **Single window** — new widget replaces current one

## License

MIT
