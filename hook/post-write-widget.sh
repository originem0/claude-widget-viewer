#!/bin/bash
# post-write-widget.sh
# Claude Code PostToolUse hook for Write tool.
# Detects widget HTML writes and launches the viewer.

FILE_PATH=$(jq -r '.tool_input.file_path // empty')

# Match both forward slash and backslash paths (Windows sends backslashes)
if [[ "$FILE_PATH" == *".claude/widgets/"*".html" ]] || [[ "$FILE_PATH" == *'.claude\widgets\'*'.html' ]]; then
  WIN_PATH=$(cygpath -w "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")
  cmd.exe /c "start /b claude-widget-viewer.exe send \"$WIN_PATH\"" </dev/null >/dev/null 2>&1
fi
exit 0
