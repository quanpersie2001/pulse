#!/bin/sh
# Pulse host detector for Claude Code (plan 0022 SS10.4).
# Reads the statusline JSON payload on stdin; touches a marker once context
# usage crosses 70% and a run is in progress, so post-tool-use.sh can tell
# the agent to checkpoint and exit continue.
payload=$(cat)
pct=$(printf '%s' "$payload" | sed -n 's/.*"used_percentage"[: ]*\([0-9]*\).*/\1/p')
if [ -n "$pct" ] && [ "$pct" -ge 70 ] 2>/dev/null && [ -e .pulse/runtime/run/current ]; then
touch .pulse/runtime/context-threshold
fi
