#!/bin/sh
# on_complete.sh — called by squad when a task is dispatched to this agent.
#
# Environment:
#   $SQUAD_MESSAGE  — the full message text sent by the workflow
#
# Usage: configure hook_script: .squad/hooks/on_complete.sh in squad.yaml
# and pass the next agent name as the first positional argument if needed.
#
# This example collects a git diff summary and sends it to the next agent.

NEXT_AGENT="${1:-reviewer}"
DIFF_STAT=$(git diff --stat HEAD~1 2>/dev/null || echo "(no prior commit)")
squad-hook send "$NEXT_AGENT" "Task completed: $DIFF_STAT"
