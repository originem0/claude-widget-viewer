#!/bin/bash
# widget-daemon-start.sh
# Claude Code SessionStart hook: starts the widget viewer daemon in background.
# The daemon preheats WebView2 and listens for widget commands via Named Pipe.

claude-widget-viewer listen &
disown
exit 0
