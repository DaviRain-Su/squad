<h1 align="center">squad</h1>

<p align="center"><strong>Multi-AI-agent terminal collaboration via simple CLI commands.</strong></p>

<p align="center">
  <a href="https://github.com/mco-org/squad/stargazers"><img src="https://img.shields.io/github/stars/mco-org/squad?style=flat-square&color=f59e0b" alt="GitHub stars" /></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-MIT-22c55e?style=flat-square" alt="License: MIT" /></a>
  <img src="https://img.shields.io/badge/Rust-1.77%2B-orange?style=flat-square&logo=rust&logoColor=white" alt="Rust 1.77+" />
  <img src="https://img.shields.io/badge/Platforms-4%20supported-7c3aed?style=flat-square" alt="4 supported platforms" />
</p>

<p align="center">squad lets multiple AI CLI agents communicate through shell commands + SQLite.<br/>No daemon, no background processes — every command is a one-shot operation.</p>

<p align="center">English | <a href="./README.zh-CN.md">简体中文</a></p>

<table align="center">
  <tr>
    <td align="center"><a href="https://github.com/anthropics/claude-code"><img src="https://github.com/anthropics.png?size=96" alt="Claude Code" width="48" /></a></td>
    <td align="center"><a href="https://github.com/google-gemini/gemini-cli"><img src="https://github.com/google-gemini.png?size=96" alt="Gemini CLI" width="48" /></a></td>
    <td align="center"><a href="https://github.com/openai/codex"><img src="https://github.com/openai.png?size=96" alt="Codex CLI" width="48" /></a></td>
    <td align="center"><a href="https://github.com/sst/opencode"><img src="https://raw.githubusercontent.com/sst/opencode/master/packages/console/app/src/asset/brand/opencode-logo-light-square.svg" alt="OpenCode" width="48" /></a></td>
  </tr>
  <tr>
    <td align="center"><strong>Claude Code</strong></td>
    <td align="center"><strong>Gemini CLI</strong></td>
    <td align="center"><strong>Codex CLI</strong></td>
    <td align="center"><strong>OpenCode</strong></td>
  </tr>
  <tr>
    <td align="center"><code>claude</code></td>
    <td align="center"><code>gemini</code></td>
    <td align="center"><code>codex</code></td>
    <td align="center"><code>opencode</code></td>
  </tr>
</table>

> One slash command. Multiple agents collaborating in real-time.
>
> Assign a manager, spin up workers, add an inspector — each in its own terminal, communicating through SQLite.

---

## Install

```bash
# Homebrew (macOS)
brew install mco-org/tap/squad

# Windows (GitHub Releases)
# 1. Download squad-x86_64-pc-windows-msvc.zip
# 2. Extract squad.exe to a folder like C:\Tools\squad
# 3. Add that folder to PATH

# Or download another prebuilt binary from GitHub Releases
# https://github.com/mco-org/squad/releases

# Or build from source
cargo install --git https://github.com/mco-org/squad.git
```

## Quick Start

```bash
# Install /squad slash command for your AI tools
squad setup

# Initialize workspace in your project
squad init

# In any AI CLI terminal — just use the slash command
/squad manager      # terminal 1
/squad worker       # terminal 2
/squad inspector    # terminal 3
```

That's it. Each agent joins, reads its role instructions, and enters a work loop that checks for messages. The manager breaks down your goal and assigns tasks to workers.

## Usage Flow

```
You (human)
  │
  ├── Terminal 1: /squad manager
  │     Manager joins, asks you for the goal,
  │     breaks it into tasks, assigns to workers.
  │
  ├── Terminal 2: /squad worker
  │     Worker joins, checks for tasks via squad receive,
  │     executes assigned work, reports back.
  │
  └── Terminal 3: /squad worker
        Auto-assigned as worker-2 (ID conflict resolved automatically).
        Same behavior — checks, executes, reports.
```

Multiple agents with the same role get unique IDs automatically (`worker`, `worker-2`, `worker-3`).

## Commands

| Command | Description |
|---------|-------------|
| `squad init` | Initialize workspace, create `.squad/`, add `.squad/` to `.gitignore`, and append squad guidance to `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` if missing |
| `squad join <id> [--role <role>]` | Join as agent (auto-suffixes if ID is taken) |
| `squad leave <id>` | Remove agent |
| `squad agents` | List online agents |
| `squad send <from> <to> <message>` | Send message (`@all` to broadcast, or `squad send --file <path-or-> <from> <to>` to read from file/stdin) |
| `squad receive <id> [--wait]` | Check inbox (`--wait` is for manual/debug use) |
| `squad pending` | Show all unread messages |
| `squad history [agent] [--from <id>] [--to <id>] [--since <RFC3339\|unix-seconds>]` | Show timestamped message history with optional filters |
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

`squad init` does more than create `.squad/`: it also appends `.squad/` to `.gitignore` and adds a short squad collaboration section to `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` when those files do not already contain one.

## How It Works

Agents communicate through a shared SQLite database (`.squad/messages.db`). Each agent runs in its own terminal and uses CLI commands to send and receive messages.

```
Terminal 1 (manager)          Terminal 2 (worker)          Terminal 3 (worker-2)
┌─────────────────────┐      ┌─────────────────────┐      ┌─────────────────────┐
│ /squad manager       │      │ /squad worker        │      │ /squad worker        │
│                      │      │ (auto-ID: worker)    │      │ (auto-ID: worker-2)  │
│                      │      │                      │      │                      │
│ squad send manager   │─────>│ squad receive worker │      │                      │
│   worker "task A"    │      │                      │      │                      │
│                      │      │                      │      │                      │
│ squad send manager   │──────────────────────────────────>│ squad receive         │
│   worker-2 "task B"  │      │                      │      │   worker-2           │
│                      │      │                      │      │                      │
│ squad receive manager│<─────│ squad send worker    │      │                      │
│                      │      │   manager "done A"   │      │                      │
│                      │      │                      │      │                      │
│                      │<──────────────────────────────────│ squad send worker-2   │
│                      │      │                      │      │   manager "done B"   │
└─────────────────────┘      └─────────────────────┘      └─────────────────────┘
```

All messages flow through SQLite — no daemon, no sockets, no background processes.

### Message Flow

Agents should use one-shot `squad receive` checks inside their work loop:

```
Agent joins
  → squad receive <id>                 ← checks once and returns
  → receives task from manager
  → executes the task
  → squad send <id> manager "done: summary..."
  → squad receive <id>                 ← checks again when ready
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
