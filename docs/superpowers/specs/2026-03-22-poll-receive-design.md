# Poll-based Receive — Fix AI Tool Compatibility

**Goal:** Replace blocking `--wait` with non-blocking `--poll` as the default receive behavior, eliminating the double-consumer race condition caused by AI tools backgrounding long-running commands.

**Problem:** `squad receive --wait` is a long-polling command that blocks until a message arrives (default 3600s). AI tools (Claude Code, Gemini, Codex) have bash execution timeouts (120-300s) that either kill or background the command. When backgrounded, the AI agent starts a second receive, creating two consumers competing for the same messages. The `receive_messages` atomic transaction (SELECT + UPDATE read=1) is single-consumer — the loser never sees the message.

**Root cause:** Long-polling conflicts with AI tools' "execute → get result → think → execute" model.

## Solution

### 1. Make `--poll` the default behavior

```bash
squad receive worker2              # default: check once, return immediately (current no-flag behavior, unchanged)
squad receive worker2 --poll 5     # check up to 5 times, 2s interval, max 10s total
squad receive worker2 --wait       # legacy long-poll, opt-in for manual/debug use only
```

`--poll N` checks for messages N times with a short interval (2s). Total runtime is bounded and short enough to never trigger bash tool timeouts. If messages arrive during polling, they're returned immediately.

The existing no-flag behavior (`squad receive worker2`) already checks once and returns — this stays unchanged and becomes the recommended default for AI agents.

### 2. Split receive transaction (safety net)

Change `receive_messages` from atomic (SELECT+UPDATE in one transaction) to two-phase:
1. SELECT unread messages (no transaction)
2. Print/return messages
3. UPDATE mark as read

This ensures that if two consumers exist (despite best efforts), both can see the message. Duplicate delivery is acceptable; message loss is not.

### 3. Update slash command templates

Change from:
```
After completing any task, always run `squad receive <your-id> --wait` to wait for the next message.
```

To:
```
After completing any task, check for new messages:
  squad receive <your-id>
If no messages, continue with other work or check again shortly.
```

Remove the `--wait` retry instruction entirely.

## Behavior Matrix

| Command | Behavior | Duration | AI-safe |
|---------|----------|----------|---------|
| `squad receive <id>` | Check once, return | instant | yes |
| `squad receive <id> --poll 5` | Check 5x, 2s interval | max 10s | yes |
| `squad receive <id> --poll` | Check 5x (default) | max 10s | yes |
| `squad receive <id> --wait` | Block until message or timeout | up to 3600s | no (debug only) |

## Files Modified

| File | Changes |
|------|---------|
| `src/main.rs` | Add `--poll` flag parsing, implement poll loop |
| `src/store.rs` | Split `receive_messages` transaction |
| `src/setup.rs` | Update slash command templates (both MD and TOML) |
| `src/roles/worker.md` | Remove `--wait` from role instructions |
| `src/roles/manager.md` | Remove `--wait` from role instructions |
| `src/roles/inspector.md` | Remove `--wait` from role instructions |

## Out of Scope

- Removing `--wait` entirely (keep for backward compat / debug)
- Changing `--wait` timeout default
- PID-based locking
