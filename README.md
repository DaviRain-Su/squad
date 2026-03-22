<div align="center">

# squad

**Multi-AI-agent terminal collaboration via simple CLI commands.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.77+-orange.svg)](https://www.rust-lang.org/)
[![GitHub stars](https://img.shields.io/github/stars/mco-org/squad)](https://github.com/mco-org/squad/stargazers)

squad lets multiple AI CLI agents communicate through shell commands + SQLite.
No daemon, no background processes — every command is a one-shot operation.

English | [简体中文](README.zh-CN.md)

### Supported Platforms

| <img src="https://cdn.simpleicons.org/anthropic/white" width="28"> | <img src="https://cdn.simpleicons.org/google/white" width="28"> | <img src="https://cdn.simpleicons.org/openai/white" width="28"> | <img src="https://cdn.simpleicons.org/square/white" width="28"> |
|:---:|:---:|:---:|:---:|
| **Claude Code** | **Gemini CLI** | **Codex CLI** | **OpenCode** |
| `claude` | `gemini` | `codex` | `opencode` |

</div>

---

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

That's it. Each agent joins, reads its role instructions, and enters a work loop waiting for messages. The manager breaks down your goal and assigns tasks to workers.

## Usage Flow

```
You (human)
  │
  ├── Terminal 1: /squad manager
  │     Manager joins, asks you for the goal,
  │     breaks it into tasks, assigns to workers.
  │
  ├── Terminal 2: /squad worker
  │     Worker joins, waits for tasks via squad receive --wait,
  │     executes assigned work, reports back.
  │
  └── Terminal 3: /squad worker
        Auto-assigned as worker-2 (ID conflict resolved automatically).
        Same behavior — waits, executes, reports.
```

Multiple agents with the same role get unique IDs automatically (`worker`, `worker-2`, `worker-3`).

## Commands

| Command | Description |
|---------|-------------|
| `squad init` | Initialize workspace (creates `.squad/` directory) |
| `squad join <id> [--role <role>]` | Join as agent (auto-suffixes if ID is taken) |
| `squad leave <id>` | Remove agent |
| `squad agents` | List online agents |
| `squad send <from> <to> <message>` | Send message (`@all` to broadcast) |
| `squad receive <id> [--wait]` | Check inbox (`--wait` blocks until message arrives) |
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
| Gemini CLI | `gemini` | `~/.gemini/commands/squad.toml` |
| Codex CLI | `codex` | `~/.codex/prompts/squad.md` |
| OpenCode | `opencode` | `~/.config/opencode/commands/squad.md` |

Once installed, use `/squad <role>` in any project where `squad init` has been run.

## How It Works

Agents communicate through a shared SQLite database (`.squad/messages.db`). Each agent runs in its own terminal and uses CLI commands to send and receive messages.

```
Terminal 1 (manager)          Terminal 2 (worker)          Terminal 3 (worker-2)
┌─────────────────────┐      ┌─────────────────────┐      ┌─────────────────────┐
│ /squad manager       │      │ /squad worker        │      │ /squad worker        │
│                      │      │ (auto-ID: worker)    │      │ (auto-ID: worker-2)  │
│                      │      │                      │      │                      │
│ squad send manager   │─────>│ squad receive worker │      │                      │
│   worker "task A"    │      │   --wait             │      │                      │
│                      │      │                      │      │                      │
│ squad send manager   │──────────────────────────────────>│ squad receive         │
│   worker-2 "task B"  │      │                      │      │   worker-2 --wait    │
│                      │      │                      │      │                      │
│ squad receive manager│<─────│ squad send worker    │      │                      │
│   --wait             │      │   manager "done A"   │      │                      │
│                      │      │                      │      │                      │
│                      │<──────────────────────────────────│ squad send worker-2   │
│                      │      │                      │      │   manager "done B"   │
└─────────────────────┘      └─────────────────────┘      └─────────────────────┘
```

All messages flow through SQLite — no daemon, no sockets, no background processes.

### Message Flow

Agents use `squad receive --wait` to block until messages arrive:

```
Agent joins
  → squad receive <id> --wait          ← blocks until message arrives
  → receives task from manager
  → executes the task
  → squad send <id> manager "done: summary..."
  → squad receive <id> --wait          ← blocks again for next task
```

### ID Auto-Suffix

When multiple agents join with the same ID, squad automatically assigns unique IDs:

```bash
squad join worker    # → Joined as worker
squad join worker    # → ID 'worker' was taken. Joined as worker-2
squad join worker    # → ID 'worker' was taken. Joined as worker-3
```

This is handled server-side (atomic `INSERT OR IGNORE`), so even simultaneous joins from different terminals are safe.

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
