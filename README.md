[中文](README.zh-CN.md)

# squad

**Multi-AI-agent terminal collaboration via simple CLI commands.**

squad lets multiple AI CLI agents (Claude Code, Gemini, Codex, etc.) communicate through shell commands backed by SQLite. No daemon, no background processes — every command is a one-shot operation.

## Quick Start

```bash
# Install
cargo install --path .

# Install /squad slash command for your AI tools
squad setup

# Initialize workspace in your project
squad init

# In any AI CLI terminal — just use the slash command
/squad manager      # terminal 1
/squad worker       # terminal 2
/squad inspector    # terminal 3
```

Or manually:

```bash
squad join manager --role manager
squad send manager worker "implement auth module with JWT"
squad receive worker --wait
```

## Commands

| Command | Description |
|---------|-------------|
| `squad init` | Initialize workspace (creates `.squad/` directory) |
| `squad join <id> [--role <role>]` | Join as agent (role defaults to id) |
| `squad leave <id>` | Remove agent |
| `squad agents` | List online agents |
| `squad send <from> <to> <message>` | Send message (`@all` to broadcast) |
| `squad receive <id> [--wait] [--timeout N]` | Check inbox (`--wait` blocks until message, default 120s) |
| `squad pending` | Show all unread messages |
| `squad history [agent]` | Show all messages including read |
| `squad roles` | List available roles |
| `squad teams` | List available teams |
| `squad team <name>` | Show team template |
| `squad setup [platform]` | Install `/squad` slash command for AI tools |
| `squad setup --list` | List supported platforms and status |
| `squad clean` | Clear all state |

## Setup

Install the `/squad` slash command for your AI tools:

```bash
squad setup           # auto-detect and install for all found tools
squad setup claude    # install only for Claude Code
squad setup --list    # show supported platforms
```

Supported platforms:

| Platform | Binary | Command location |
|----------|--------|-----------------|
| Claude Code | `claude` | `~/.claude/commands/squad.md` |
| Gemini CLI | `gemini` | `~/.gemini/antigravity/global_workflows/squad.md` |
| Codex CLI | `codex` | `~/.codex/prompts/squad.md` |

Once installed, use `/squad <role>` in any project where `squad init` has been run.

## How It Works

Agents communicate through a shared SQLite database (`.squad/messages.db`). Each agent runs in its own terminal and uses CLI commands to send and receive messages.

```
Terminal 1 (manager)          Terminal 2 (worker)          Terminal 3 (inspector)
┌─────────────────────┐      ┌─────────────────────┐      ┌─────────────────────┐
│ squad join manager   │      │ squad join worker    │      │ squad join inspector │
│                      │      │                      │      │                      │
│ squad send manager   │─────>│ squad receive worker │      │                      │
│   worker "task..."   │      │   --wait             │      │                      │
│                      │      │                      │      │                      │
│ squad receive manager│<─────│ squad send worker    │      │                      │
│   --wait             │      │   manager "done..."  │      │                      │
│                      │      │                      │      │                      │
│ squad send manager   │─────────────────────────────────>│ squad receive         │
│   inspector "review" │      │                      │      │   inspector --wait   │
└─────────────────────┘      └─────────────────────┘      └─────────────────────┘
```

All messages flow through SQLite — no daemon, no sockets, no background processes.

### The `--wait` Pattern

When an agent finishes its work, it calls `squad receive <id> --wait` to block until the next message arrives. This creates a natural event loop:

```
Agent completes task
  → squad send <id> manager "done: summary..."
  → squad receive <id> --wait              ← blocks here
  → receives new task or feedback
  → works on it
  → repeat
```

## Role Templates

Roles are `.md` files in `.squad/roles/` that define agent behavior. Three are built in:

- **manager** — breaks down goals, assigns tasks, coordinates review
- **worker** — executes tasks, reports results
- **inspector** — reviews code, sends PASS/FAIL verdicts

Create custom roles by adding `.md` files to `.squad/roles/`:

```bash
echo "You are a database specialist..." > .squad/roles/dba.md
squad join db-expert --role dba
```

## Team Templates

Teams are YAML files in `.squad/teams/` that define which roles are needed:

```yaml
# .squad/teams/dev.yaml
name: dev
roles:
  manager:
    prompt_file: manager
  worker:
    prompt_file: worker
  inspector:
    prompt_file: inspector
```

View a team's setup instructions:

```bash
squad team dev
```

## Broadcast

Send a message to all agents at once:

```bash
squad send manager @all "API contract changed, update your implementations"
```

## Requirements

- Rust 1.77+ (for building)
- macOS or Linux

## License

MIT
