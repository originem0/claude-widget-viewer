#!/bin/bash
# post-write-widget.sh
# Claude Code PostToolUse hook for Write tool.
# Detects widget HTML writes and sends them to the viewer.
# Uses cmd.exe to fully detach from Claude Code's process tree.

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only trigger for .claude/widgets/*.html files
if [[ "$FILE_PATH" == *".claude/widgets/"*".html" ]]; then
  # Convert to Windows path for cmd.exe
  WIN_PATH=$(cygpath -w "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")
  cmd.exe /c "start /b claude-widget-viewer.exe send \"$WIN_PATH\"" </dev/null >/dev/null 2>&1
fi
exit 0
