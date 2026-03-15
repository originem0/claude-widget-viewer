[English](README.md) | [中文](README.zh-CN.md)

# claude-widget-viewer

轻量级原生 widget 渲染器，将 [claude.ai 的生成式 UI](https://claude.com/blog/claude-builds-visuals) 带到 Claude Code。Rust + WebView2，单文件 756KB。

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

## 前提条件

- **Windows 11**（WebView2 预装）
- **Rust 1.77+** MSVC 工具链
- **MSVC Build Tools** + **Windows SDK**（编译 wry/WebView2 绑定需要）
- **jq**（hook 脚本解析 JSON 用）
- **Claude Code** CLI

### 安装前提（scoop）

```bash
scoop install rust jq
winget install Microsoft.VisualStudio.2022.BuildTools --silent --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --includeRecommended"
```

### Cargo 镜像（国内加速）

如果 `cargo build` 卡在 "Updating crates.io index"，在 `~/.cargo/config.toml` 配置镜像：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
```

## 构建

```bash
git clone git@github.com:originem0/claude-widget-viewer.git
cd claude-widget-viewer
cargo build --release
```

产物在 `target/release/claude-widget-viewer.exe`（约 756KB）。

## 安装

### 1. 将二进制放入 PATH

```bash
cp target/release/claude-widget-viewer.exe <PATH中的某个目录>/
# 例如 scoop shims：
cp target/release/claude-widget-viewer.exe ~/scoop/shims/
```

### 2. 部署 hook 脚本

```bash
mkdir -p ~/.claude/hooks
cp hook/widget-daemon-start.sh ~/.claude/hooks/
cp hook/post-write-widget.sh ~/.claude/hooks/
```

### 3. 配置 Claude Code hooks

在 `~/.claude/settings.json` 中添加（合并到已有的 `"hooks"` 键）：

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

### 4. 安装 skill（可选）

将 `claude/SKILL.md` 复制到 skills 目录，Claude 在遇到可视化请求时会自动加载 widget 指令：

```bash
mkdir -p ~/.claude/skills/widget-viewer
cp claude/SKILL.md ~/.claude/skills/widget-viewer/SKILL.md
```

不装 skill 也能用，手动把 `claude/widget-protocol.md` 的内容加到项目 `CLAUDE.md` 里即可。

## 使用

### 自动模式（通过 Claude Code hooks）

启动新的 Claude Code session，daemon 在后台自动启动。向 Claude 发出可视化请求：

> "画一个过去一周的温度折线图"

Claude 写入 `.claude/widgets/temperature_chart.html` → hook 触发 → 窗口弹出 Chart.js 渲染。

### 手动模式

```bash
# 独立模式（打开窗口 + 监听文件变更）
claude-widget-viewer show path/to/widget.html

# daemon 模式（隐藏窗口，预热 WebView2，监听 Named Pipe）
claude-widget-viewer listen

# 发送 widget 到运行中的 daemon（无 daemon 时自动降级为 show）
claude-widget-viewer send path/to/widget.html

# 停止 daemon
claude-widget-viewer stop
```

### 热重载

viewer 运行时编辑 widget HTML 文件，变更实时生效（200ms 防抖）。适合迭代调整 widget 设计。

## Widget HTML 格式

Widget 是原始 HTML 片段。不需要 `<!DOCTYPE>`、`<html>`、`<head>`、`<body>` 标签。结构顺序：style → 内容 → script。

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

### CSS 变量

Viewer 注入了匹配 claude.ai 风格的设计系统，使用以下变量：

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

浅色/深色模式通过 `prefers-color-scheme` 自动切换。

### CDN

仅允许 `https://cdnjs.cloudflare.com`。CDN 脚本始终使用 `onload` + fallback 模式。

## 架构

```
src/
  main.rs       CLI 入口，4 个子命令 (show/listen/send/stop)
  viewer.rs     winit 窗口 + wry WebView，事件循环
  protocol.rs   wry:// 自定义协议处理器
  shell.rs      HTML shell 生成，base64 注入
  watcher.rs    文件监听，200ms 防抖 (notify crate)
  ipc.rs        Windows Named Pipe 服务端/客户端 (windows-sys)

assets/
  design-system.css   CSS 变量 (light/dark)，SVG 类
  morphdom.min.js     DOM diffing 库 (12KB，编译时内嵌)
```

所有资源通过 `include_str!` 在编译时内嵌——运行时零外部文件依赖。

### IPC 协议

daemon 监听 `\\.\pipe\claude-widget-viewer-{pid}`，消息为 JSON，每次连接一条：

```json
{"type":"LoadWidget","file":"C:/path/to/widget.html","title":"my_chart"}
{"type":"UpdateWidget","html":"<div>增量更新</div>"}
{"type":"Show"}
{"type":"Close"}
```

`send` 子命令处理所有管道通信。Hook 脚本从不直接操作 Named Pipe。

## 已知限制

- **仅限 Windows** — 使用 WebView2（macOS 需要移植到 WKWebView）
- **无流式渲染** — Claude 生成完整 HTML 后一次性写入。`UpdateWidget` 消息已预留给未来的 MCP 流式支持
- **无 sendPrompt()** — widget 暂时不能向 Claude 发回消息（stub 已存在，需要 MCP 集成）
- **单窗口** — 新 widget 替换当前显示的内容

## License

MIT
