# squad

**Multi-AI-agent terminal collaboration — let Claude Code, Codex, Gemini, and others work together automatically.**

squad runs a local daemon that routes messages between AI CLI agents over MCP, hook scripts, or watched files, coordinating them through loop, pipeline, or parallel workflows.

---

## Quick Start

```bash
# 1. Install from source (requires Rust / cargo)
git clone https://github.com/mco-org/squad.git
cd squad
./install.sh          # builds & installs squad, squad-mcp, squad-hook

# Or install directly with cargo:
cargo install --git https://github.com/mco-org/squad

# 2. Initialize a workspace
cd my-project
squad init

# 3. Register the squad MCP server (Claude Code)
squad setup cc --update-claude-md

# 4. Edit squad.yaml to describe your agents and workflow
$EDITOR squad.yaml

# 5. Start the daemon
squad start

# 6. Watch it run
squad watch
```

---

## Core Concepts

### Daemon

The `squad` daemon is a background process that runs in your workspace. It manages agent registration, message routing, heartbeat tracking, workflow state, and persistence. All agents communicate through the daemon's Unix socket (`.squad/squad.sock`).

### MCP Server

`squad-mcp` is a Model Context Protocol server that AI agents (Claude Code, etc.) connect to. It exposes four tools:

| Tool | Description |
|------|-------------|
| `send_message` | Send a message to another agent |
| `check_inbox` | Fetch messages from the daemon mailbox |
| `mark_done` | Record task completion and advance the workflow |
| `send_heartbeat` | Notify the daemon you are alive |

### Workflow

A workflow is a sequence (or set) of steps defined in `squad.yaml`. Each step assigns an action to an agent. The workflow engine routes execution between steps based on agent `mark_done` outcomes.

### Adapters

Adapters are how the daemon talks to non-MCP agents:

| Adapter | Mechanism |
|---------|-----------|
| `mcp` (default) | Agent connects via `squad-mcp` MCP server |
| `hook` | Daemon calls a shell script with `$SQUAD_MESSAGE` |
| `watch` | Daemon writes a file; agent reads and overwrites it |

---

## Configuration Reference

Full `squad.yaml` example:

```yaml
project: my-project

heartbeat_timeout_seconds: 30

persistence:
  enabled: false

recovery:
  on_agent_offline: reconnect   # reconnect | restart | notify | ignore
  reconnect_attempts: 3
  reconnect_interval_seconds: 5

agents:
  builder:
    adapter: mcp                # mcp | hook | watch

  reviewer:
    adapter: hook
    hook_script: .squad/hooks/reviewer.sh

  codex:
    adapter: watch
    watch_file: .squad/codex-output.txt

workflow:
  mode: loop                    # loop | pipeline | parallel
  start_at: implement
  max_iterations: 10
  on_timeout: stop              # stop | notify | restart
  timeout_seconds: 300

  steps:
    - id: implement
      agent: builder
      action: implement
      message: "Goal: {goal}\n\nPrevious output:\n{previous_output}"
      on_pass: review
      on_fail: implement

    - id: review
      agent: reviewer
      action: review
      message: "Review iteration {iteration}:\n{previous_output}"
      next: done
```

### Template Variables

Step `message` (alias: `prompt`) fields support these variables:

| Variable | Value |
|----------|-------|
| `{goal}` | Initial goal string passed to the workflow |
| `{previous_output}` | Summary from the previous step's `mark_done` |
| `{iteration}` | Current iteration count |

---

## CLI Commands

| Command | Description |
|---------|-------------|
| `squad init` | Create `squad.yaml` template and example hook scripts |
| `squad init --fresh` | Same as `init` but also clears history first |
| `squad start` | Start the daemon in the background |
| `squad stop` | Gracefully stop the daemon |
| `squad status` | Show daemon status and agent health |
| `squad log` | Print the audit log |
| `squad log --tail N` | Show last N audit entries |
| `squad log --filter key=val` | Filter log by field |
| `squad history` | Show workflow session history summary |
| `squad clean` | Delete runtime state (messages, sessions, audit) |
| `squad watch` | Open the live TUI dashboard |

---

## TUI Dashboard

Press `q` to quit.

```
┌─ squad — my-project ─────────────────────────────────────────────────────┐
│ mode: loop  step: implement  iteration: 3/10  running: true              │
├──────────────────────────────────┬───────────────────────────────────────┤
│ Agents                           │ Messages                              │
│                                  │                                       │
│ builder    [working] online      │ workflow -> builder                   │
│ reviewer   [idle]    online      │   Goal: refactor auth module          │
│                                  │                                       │
│                                  │ workflow -> reviewer                  │
│                                  │   Review iteration 2: ...             │
│                                  │                                       │
└──────────────────────────────────┴───────────────────────────────────────┘
```

---

## Supported Agents

Any AI CLI that can act as an MCP client works out of the box:

| Agent | Adapter | Notes |
|-------|---------|-------|
| Claude Code (`claude`) | `mcp` | Add `squad-mcp` as MCP server in `~/.claude/settings.json` |
| OpenAI Codex CLI | `hook` or `watch` | Use a shell script or watched file |
| Gemini CLI | `hook` or `watch` | Same as Codex |
| Any CLI tool | `hook` | Run any command with `$SQUAD_MESSAGE` |
| File-based agents | `watch` | Agent reads/writes a shared file |

### Connecting Claude Code via MCP

Add to `~/.claude/settings.json` (or workspace `.claude/settings.json`):

```json
{
  "mcpServers": {
    "squad": {
      "command": "squad-mcp",
      "env": {
        "SQUAD_AGENT_ID": "builder"
      }
    }
  }
}
```

---

## Runtime Files

All runtime state lives in `.squad/` (auto-created, gitignored):

```
.squad/
  squad.sock       Unix socket (daemon IPC)
  daemon.pid       Daemon process ID
  state.json       Workflow state
  session.json     Session metadata
  messages.log     Live message feed (TUI)
  messages.db      Persisted message store
  audit.log        Full audit trail
  hooks/
    on_complete.sh Example completion hook
    codex.sh       Example Codex hook
```

---

## Documentation

- [Getting Started](docs/getting-started.md) — full walkthrough with two agents
- [Workflow Modes](docs/workflow-modes.md) — loop, pipeline, parallel
- [Adapters](docs/adapters.md) — mcp, hook, watch
- [CLI Reference](docs/cli-reference.md) — all commands and flags
- [squad.yaml Reference](docs/squad-yaml.md) — complete config reference
