#!/bin/bash
# widget-daemon-start.sh
# Optional: manually start the daemon to prewarm WebView2.
# NOT required — the PostToolUse hook auto-launches on first widget write.

cmd.exe /c "start /b claude-widget-viewer.exe listen" </dev/null >/dev/null 2>&1
exit 0
