#!/bin/sh
# Pulse host detector for Claude Code (plan 0022 SS10.4).
if [ -e .pulse/runtime/context-threshold ]; then
rm -f .pulse/runtime/context-threshold
printf '%s\n' '{"decision":"continue","reason":"Context >=70%: pulse checkpoint then exit {\"status\":\"continue\"}"}'
fi
