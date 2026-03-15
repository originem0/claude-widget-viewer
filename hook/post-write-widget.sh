#!/bin/bash
# post-write-widget.sh
# Claude Code PostToolUse hook for Write tool.
# Viewer handles its own process detachment — hook just calls send.

FILE_PATH=$(jq -r '.tool_input.file_path // empty')

if [[ "$FILE_PATH" == *".claude/widgets/"*".html" ]] || [[ "$FILE_PATH" == *'.claude\widgets\'*'.html' ]]; then
  claude-widget-viewer send "$FILE_PATH"
fi
exit 0
