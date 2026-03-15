[English](README.md) | [中文](README.zh-CN.md)

# claude-widget-viewer

轻量级原生 widget 渲染器，将 [claude.ai 的生成式 UI](https://claude.com/blog/claude-builds-visuals) 带到 Claude Code。Rust + WebView2，单文件 ~800KB。响应式布局——图表和图解充满窗口，随窗口缩放。

Claude Code 在终端里无法渲染 HTML。这个工具解决这个问题：Claude 把 widget HTML 写入文件，hook 检测到写入，原生 WebView2 窗口弹出渲染结果——图表、图解、交互控件，全部运行在真正的浏览器引擎中。

```
Claude Code ─── Write ──→ .claude/widgets/chart.html
                                    │
                          PostToolUse hook
                                    │
                          claude-widget-viewer.exe send chart.html
                                    │
                              Named Pipe IPC
                                    │
                          daemon (预热的 WebView2)
                                    │
                          原生窗口渲染 widget
```

## 快速安装

**运行要求：** Windows 10/11 + [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)（Win11 预装，Win10 可下载安装）。

**一行安装**（PowerShell）：

```powershell
irm https://raw.githubusercontent.com/originem0/claude-widget-viewer/main/install.ps1 | iex
```

或克隆后本地运行：

```powershell
git clone https://github.com/originem0/claude-widget-viewer.git
cd claude-widget-viewer
powershell -ExecutionPolicy Bypass -File install.ps1
```

安装脚本自动完成：下载预编译二进制、部署 hooks、配置 `settings.json`、安装 skill。无需任何构建工具。

**卸载：**

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -Uninstall
```

## 从源码构建

仅当你需要修改代码或自定义构建配置时才需要。

**构建前提：** Rust 1.85+（MSVC 工具链，edition 2024）、MSVC Build Tools、Windows SDK。

```bash
# 安装构建工具（如未安装）
scoop install rust
winget install Microsoft.VisualStudio.2022.BuildTools --silent --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --includeRecommended"

# 构建
git clone https://github.com/originem0/claude-widget-viewer.git
cd claude-widget-viewer
cargo build --release
# 产物: target/release/claude-widget-viewer.exe (~800KB)
```

<details>
<summary>国内 Cargo 镜像加速</summary>

如果 `cargo build` 卡在 "Updating crates.io index"，在 `~/.cargo/config.toml` 配置：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
```
</details>

构建完成后运行 `install.ps1` 自动部署，或手动配置：

<details>
<summary>手动配置</summary>

1. 将 `target/release/claude-widget-viewer.exe` 放到 PATH 中
2. 将 `hook/*.sh` 复制到 `~/.claude/hooks/`
3. 在 `~/.claude/settings.json` 中合并 hooks 配置：

```json
{
  "hooks": {
    "PostToolUse": [
      { "matcher": "Write", "hooks": [{ "type": "command", "command": "bash ~/.claude/hooks/post-write-widget.sh" }] }
    ]
  }
}
```

4.（可选）将 `claude/SKILL.md` 复制到 `~/.claude/skills/widget-viewer/SKILL.md`
</details>

## 使用

### 自动模式（通过 Claude Code hooks）

启动新的 Claude Code session，daemon 在后台自动启动。向 Claude 发出可视化请求：

> "画一个过去一周的温度折线图"

Claude 写入 `.claude/widgets/temperature_chart.html` → hook 触发 → 窗口弹出 Chart.js 渲染。

### 手动模式

```bash
claude-widget-viewer show path/to/widget.html    # 打开 + 监听文件变更
claude-widget-viewer listen                       # daemon 模式（预热）
claude-widget-viewer send path/to/widget.html     # 发送到 daemon
claude-widget-viewer stop                         # 停止 daemon
claude-widget-viewer hook                         # 从 stdin 处理 hook JSON
```

### 热重载

viewer 运行时编辑 widget HTML 文件，变更实时生效（200ms 防抖）。

## Widget HTML 格式

Widget 是原始 HTML 片段。不需要 `<!DOCTYPE>`、`<html>`、`<head>`、`<body>`。结构：style → 内容 → script。布局自适应——widget 充满窗口并跟随缩放。

```html
<style>
  .widget-card { padding: var(--spacing-lg); background: var(--color-bg-secondary); border-radius: var(--border-radius-lg); }
</style>
<div class="widget-card">
  <div class="chart-wrap"><canvas id="chart"></canvas></div>
</div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js" onload="initChart()"></script>
<script>
function initChart() {
  var cs = getComputedStyle(document.documentElement);
  new Chart(document.getElementById('chart'), {
    type: 'line',
    data: { /* ... */ },
    options: { responsive: true, maintainAspectRatio: true }
  });
}
if (window.Chart) initChart();
</script>
```

### CSS 变量

| 类别 | 变量 |
|------|------|
| 文字 | `--color-text-primary`, `--color-text-secondary` |
| 背景 | `--color-bg-primary`, `--color-bg-secondary` |
| 边框 | `--color-border`, `--color-border-light` |
| 语义色 | `--color-blue`, `--color-green`, `--color-amber`, `--color-red` |
| 分类色 | `--color-purple`, `--color-teal`, `--color-coral`, `--color-pink` |
| 间距 | `--spacing-xs` (4px) 到 `--spacing-xl` (32px) |
| 圆角 | `--border-radius-sm` / `md` / `lg` |
| 字体 | `--font-sans`, `--font-mono` |

浅色/深色模式通过 `prefers-color-scheme` 自动切换。允许的 CDN：`https://cdnjs.cloudflare.com`、`https://cdn.jsdelivr.net`、`https://unpkg.com`。字体：`https://fonts.googleapis.com`。

## 架构

```
src/
  main.rs       CLI 入口，5 个子命令 (show/listen/send/stop/hook)
  viewer.rs     winit 窗口 + wry WebView，事件循环
  shell.rs      HTML shell 生成，侧边栏布局，base64 注入
  watcher.rs    文件监听，200ms 防抖 (notify crate)
  ipc.rs        Windows Named Pipe 服务端/客户端 (windows-sys)
```

所有资源通过 `include_str!` 编译时内嵌——运行时零外部文件依赖。daemon 通过 `\\.\pipe\claude-widget-viewer-{pid}` 通信（JSON 消息）。

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `WIDGET_VIEWER_IDLE_TIMEOUT_SECS` | `1800`（30 分钟） | daemon 空闲超过此秒数后自动退出 |
| `WIDGET_VIEWER_IDLE_POLL_SECS` | `60` | 空闲计时器轮询间隔 |
| `WIDGET_VIEWER_STATE_DIR` | `~/.claude/` | 覆盖 pipe 状态目录（测试用） |

## 开发

```bash
cargo test                    # 单元 + 集成测试（无需 GUI）
cargo test -- --ignored       # 需要显示服务器的测试
```

测试使用 `WIDGET_VIEWER_STATE_DIR` 配合每个测试的临时目录实现完全隔离——不会写入 `~/.claude/`。

## 已知限制

- **仅限 Windows** — 使用 WebView2（macOS 需移植到 WKWebView）
- **无流式渲染** — Claude 写完整 HTML 后一次性渲染（`UpdateWidget` IPC 消息已预留给未来 MCP 支持）
- **无 sendPrompt()** — widget 暂不能向 Claude 发回消息（stub 已存在，需 MCP 集成）

## License

MIT
