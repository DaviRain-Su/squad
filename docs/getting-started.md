# Getting Started with squad

This guide walks you through setting up a two-agent collaboration loop using Claude Code and Codex CLI.

## Prerequisites

- Rust toolchain (`cargo`)
- Claude Code (`claude`) installed
- A terminal

## Step 1 — Build and Install

```bash
git clone https://github.com/your-org/squad
cd squad
cargo install --path .
```

This installs three binaries:

- `squad` — main CLI
- `squad-mcp` — MCP server for AI agents
- `squad-hook` — helper for hook-based agents

## Step 2 — Initialize a Workspace

Navigate to your project directory and run:

```bash
cd ~/my-project
squad init
```

This creates:

- `squad.yaml` — configuration template
- `.squad/hooks/` — example hook scripts

## Step 3 — Configure Two Agents in a Loop

Edit `squad.yaml`:

```yaml
project: my-project

heartbeat_timeout_seconds: 30

recovery:
  on_agent_offline: reconnect
  reconnect_attempts: 3
  reconnect_interval_seconds: 5

agents:
  cc:
    adapter: mcp

  codex:
    adapter: hook
    hook_script: .squad/hooks/codex.sh

workflow:
  mode: loop
  start_at: implement
  max_iterations: 6
  on_timeout: stop
  timeout_seconds: 300

  steps:
    - id: implement
      agent: cc
      action: implement
      message: |
        Goal: {goal}

        Previous review:
        {previous_output}

        Implement the changes.
      next: review

    - id: review
      agent: codex
      action: review
      message: |
        Review the latest changes for iteration {iteration}.

        Previous output:
        {previous_output}

        Reply PASS or FAIL with notes.
      on_pass: done
      on_fail: implement
```

### How it works

1. The daemon sends `cc` (Claude Code) the goal with `{goal}` substituted.
2. `cc` implements the changes and calls `mark_done` with a summary.
3. The workflow advances to the `review` step, sending `codex` the summary.
4. If `codex` replies with "PASS", the workflow finishes. If "FAIL", it loops back to `implement`.
5. After `max_iterations` the workflow stops.

## Step 4 — Connect Claude Code via MCP

Add `squad-mcp` to Claude Code's MCP servers. Edit `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "squad": {
      "command": "squad-mcp",
      "env": {
        "SQUAD_AGENT_ID": "cc"
      }
    }
  }
}
```

The `SQUAD_AGENT_ID` tells the MCP server which agent identity to use when checking the inbox and sending heartbeats.

## Step 5 — Set Up the Codex Hook

Edit `.squad/hooks/codex.sh`:

```sh
#!/bin/sh
# Called by squad when a message arrives for `codex`.
# $SQUAD_MESSAGE contains the message content.
echo "$SQUAD_MESSAGE" | codex --quiet
# When codex finishes, send its output back to the daemon:
squad-hook send cc "$(codex output)"
```

The hook is invoked with `$SQUAD_MESSAGE` set to the message content. Use `squad-hook send <to> <message>` to deliver a reply back to the daemon.

## Step 6 — Start the Daemon

```bash
squad start
```

Check it is running:

```bash
squad status
```

Expected output:

```
running: true
socket: /path/to/my-project/.squad/squad.sock
builder (implement) [idle] health=online last_seen=0
```

## Step 7 — Watch the Workflow

Open the live TUI:

```bash
squad watch
```

Press `q` to exit.

## Troubleshooting

**Daemon won't start**

Run `squad status`. If it prints `running: false`, check that `squad.yaml` exists and is valid YAML.

**Agent shows `health=offline`**

The agent has not sent a heartbeat within `heartbeat_timeout_seconds`. Make sure:
- Claude Code has `squad-mcp` in its MCP server list.
- `SQUAD_AGENT_ID` is set to the correct agent name.
- The agent has called `check_inbox` at least once (this also sends a heartbeat).

**Hook script not called**

Verify `hook_script` path is correct relative to the workspace root, and that the script is executable (`chmod +x`).

**View audit log**

```bash
squad log --tail 20
```

## Next Steps

- [Workflow Modes](workflow-modes.md) — learn about pipeline and parallel modes
- [Adapters](adapters.md) — connect agents without MCP
- [squad.yaml Reference](squad-yaml.md) — full configuration options
