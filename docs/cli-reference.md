# CLI Reference

## squad

Main CLI for managing the squad daemon and workspace.

```
Usage: squad <init|setup|start|run|stop|status|doctor|log|history|clean|watch> [options]
```

All commands are run from your workspace directory (where `squad.yaml` lives).

---

### `squad init`

Create a `squad.yaml` template in the current directory.

```bash
squad init
```

Creates `squad.yaml` with a ready-to-use two-agent (builder + reviewer) loop template.

> **Safe by default:** if `squad.yaml` already exists, `squad init` prints a notice and
> exits without overwriting. Use `--force` to replace it.

**Flags:**

| Flag | Description |
|------|-------------|
| `--force` | Overwrite an existing `squad.yaml` and clear runtime history |
| `--fresh` | Alias for `--force` |

```bash
squad init --force   # overwrite and reset
```

---

### `squad setup <agent>`

Register the squad MCP server for an AI agent in the current workspace.

```bash
squad setup cc                        # Claude Code
squad setup cc --update-claude-md    # also append Squad protocol to CLAUDE.md
squad setup codex
squad setup gemini
squad setup qwen
squad setup --list                    # list supported agents
```

Creates or updates `.mcp.json` with:

```json
{
  "mcpServers": {
    "squad": {
      "command": "squad-mcp",
      "args": [],
      "env": { "SQUAD_AGENT_ID": "<agent>" }
    }
  }
}
```

**`--update-claude-md`** appends a "Squad Collaboration Protocol" section to `CLAUDE.md`,
documenting `check_inbox`, `send_heartbeat`, `send_message`, and `mark_done` usage.

---

### `squad start`

Start the daemon in the background.

```bash
squad start
```

- Reads `squad.yaml` from the current directory.
- Validates agent configuration.
- Creates example artifacts for watch-adapter agents if needed.
- Spawns the daemon process and waits up to 5 seconds for it to create its socket.
- If the daemon is already running, prints a notice and exits.
- If a stale socket from a previous crash is found, removes it and starts fresh.

**Requires:** `squad.yaml` to exist.

---

### `squad run <goal>`

Start the workflow by sending a goal to the running daemon.

```bash
squad run "refactor the auth module to use JWT"
squad run "add unit tests for the billing service"
```

- Resets workflow state and stores the goal string.
- Dispatches the first workflow step's message to the configured agent, with
  `{goal}` substituted by the value you provide.
- Subsequent `mark_done` calls from agents advance the workflow automatically.

**Requires:** daemon to be running (`squad start` first).

```bash
# Typical sequence
squad start
squad run "implement the feature described in SPEC.md"
squad watch
```

The goal string is available as `{goal}` in all step `message` templates.

---

### `squad doctor`

Run a health check and print a colour-coded status report.

```bash
squad doctor
```

Checks:

| Check | Green ✓ | Yellow ⚠ | Red ✗ |
|-------|---------|-----------|-------|
| Daemon | socket exists and responds | socket exists but no response | socket not found |
| `squad-mcp` | found in PATH | — | not found in PATH |
| `.mcp.json` | squad entry present and valid | file missing or squad entry absent | file is invalid JSON |

Run this whenever agents can't connect to the daemon or MCP tools fail.

---

### `squad stop`

Gracefully shut down the daemon.

```bash
squad stop
```

- Sends a shutdown request over the Unix socket.
- Waits up to 5 seconds for the socket to disappear.
- If the daemon is not running, cleans up stale socket/pid files and exits.

---

### `squad status`

Print the current daemon and agent status.

```bash
squad status
```

Example output:

```
running: true
socket: /path/to/project/.squad/squad.sock
builder (implement) [working] health=online last_seen=1711234567
reviewer (review) [idle] health=online last_seen=1711234560
```

Agents with `health=offline` are printed in red.

If the daemon is not running:

```
running: false
```

---

### `squad log`

Print the audit log.

```bash
squad log
squad log --tail 20
squad log --filter agent=builder
```

**Flags:**

| Flag | Description |
|------|-------------|
| `--tail N` | Show only the last N entries |
| `--filter key=val` | Filter entries where `key` equals `val` |

Filterable fields include: `agent`, `event`, `session_id`.

Example:

```bash
# Show last 10 entries for agent "builder"
squad log --tail 10 --filter agent=builder
```

---

### `squad history`

Print a summary of past workflow sessions.

```bash
squad history
```

Summarizes sessions from the audit log, including session IDs, event counts, and agent activity.

---

### `squad clean`

Delete all runtime state for the current workspace.

```bash
squad clean
```

Removes:
- `.squad/messages.db`
- `.squad/session.json`
- `.squad/audit.log`
- `.squad/state.json`
- `.squad/messages.log`

Does **not** stop a running daemon or remove `squad.yaml`.

---

### `squad watch`

Open the live TUI dashboard.

```bash
squad watch
```

Displays:
- Workflow progress (mode, current step, iteration count)
- Agent roster with status and health
- Live message feed (last 32 messages)

Press `q` to quit.

---

## squad-mcp

MCP server for AI agents. Normally started automatically by the agent's MCP client.

```bash
squad-mcp
```

Communicates over stdio using the Model Context Protocol. Connects to the daemon socket in the current working directory.

**Environment variables:**

| Variable | Description |
|----------|-------------|
| `SQUAD_AGENT_ID` | Agent identity used for heartbeats and inbox (default: `assistant`) |

---

## squad-hook

Helper for hook-based agents to send messages back to the daemon.

```bash
squad-hook send <to> <message>
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `<to>` | Recipient agent name |
| `<message>` | Message content string |

**Environment variables:**

| Variable | Description |
|----------|-------------|
| `$SQUAD_HOOK_FROM` | Sender identity (default: `hook`) |

**Example:**

```sh
#!/bin/sh
# Inside a hook script
squad-hook send builder "Review complete: PASS"
```
