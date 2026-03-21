#!/bin/sh
# codex.sh — invoke Codex CLI for a review task and report the result back.
#
# Environment:
#   $SQUAD_MESSAGE  — the review request sent by the workflow
#
# Usage: configure hook_script: .squad/hooks/codex.sh in squad.yaml
# The script runs codex, captures its output, and sends the result to cc.
#
# squad.yaml example:
#   agents:
#     codex:
#       adapter: hook
#       hook_script: .squad/hooks/codex.sh

RESULT=$(echo "$SQUAD_MESSAGE" | codex --quiet 2>&1)
EXIT_CODE=$?

if [ $EXIT_CODE -eq 0 ]; then
    squad-hook send cc "Review result: PASS

$RESULT"
else
    squad-hook send cc "Review result: FAIL

$RESULT"
fi
