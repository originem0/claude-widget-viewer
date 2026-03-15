#!/bin/bash
# post-write-widget.sh
# Claude Code PostToolUse hook for Write tool.
# Fix #9: direct jq pipe, no intermediate variable.

FILE_PATH=$(jq -r '.tool_input.file_path // empty')

if [[ "$FILE_PATH" == *".claude/widgets/"*".html" ]]; then
  WIN_PATH=$(cygpath -w "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")
  cmd.exe /c "start /b claude-widget-viewer.exe send \"$WIN_PATH\"" </dev/null >/dev/null 2>&1
fi
exit 0
