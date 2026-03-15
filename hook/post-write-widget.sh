#!/bin/bash
# post-write-widget.sh
# Claude Code PostToolUse hook for Write tool.
# Detects widget HTML writes and sends them to the viewer daemon.

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only trigger for .claude/widgets/*.html files
if [[ "$FILE_PATH" == *".claude/widgets/"*".html" ]]; then
  claude-widget-viewer send "$FILE_PATH" &
  disown
fi
exit 0
