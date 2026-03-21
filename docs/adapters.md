# Adapters

Adapters define how the squad daemon communicates with each agent. Configure the adapter per agent under the `agents` section of `squad.yaml`.

---

## mcp (default)

The agent connects to the daemon through the `squad-mcp` MCP server using the Model Context Protocol over stdio.

```yaml
agents:
  builder:
    adapter: mcp
```

`adapter: mcp` is the default; you can omit the `agents` entry entirely for MCP agents.

### How it works

1. The agent starts `squad-mcp` as an MCP server (configured in the agent's settings).
2. `squad-mcp` connects to the daemon socket (`.squad/squad.sock`).
3. The daemon delivers messages to the agent's mailbox.
4. The agent calls `check_inbox` to read messages, does its work, then calls `mark_done`.

### MCP Tools

| Tool | Arguments | Description |
|------|-----------|-------------|
| `send_message` | `to: string`, `content: string` | Send a message to another agent |
| `check_inbox` | _(none)_ | Fetch messages from your mailbox; also sends heartbeat |
| `mark_done` | `summary: string` | Mark current task complete and advance the workflow |
| `send_heartbeat` | _(none)_ | Notify the daemon you are active |

### Connecting Claude Code

Add to `~/.claude/settings.json` or workspace `.claude/settings.json`:

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

`SQUAD_AGENT_ID` must match the agent name in `squad.yaml`. It is used to identify the agent when sending heartbeats and checking the inbox.

---

## hook

The daemon calls a shell script when a message arrives for this agent. The script receives the message via the `$SQUAD_MESSAGE` environment variable.

```yaml
agents:
  codex:
    adapter: hook
    hook_script: .squad/hooks/codex.sh
```

`hook_script` is required for hook adapters. The path is relative to the workspace root.

### How it works

1. The workflow dispatches a message for `codex`.
2. The daemon runs `.squad/hooks/codex.sh` with `$SQUAD_MESSAGE` set to the message content.
3. If the script exits non-zero, the daemon logs an error.
4. The hook script is responsible for invoking the agent and sending results back.

### Sending results back

Use `squad-hook send <agent> <message>` from inside the hook script to deliver a reply to the daemon:

```sh
#!/bin/sh
# .squad/hooks/codex.sh
echo "$SQUAD_MESSAGE" | codex --quiet > /tmp/codex-out.txt
squad-hook send builder "$(cat /tmp/codex-out.txt)"
```

### Environment variables

| Variable | Description |
|----------|-------------|
| `$SQUAD_MESSAGE` | The message content sent to this agent |
| `$SQUAD_HOOK_FROM` | Sender identity used by `squad-hook send` (default: `hook`) |

### Example hooks

`squad init` writes two example scripts to `.squad/hooks/`:

```sh
# on_complete.sh
#!/bin/sh
squad-hook send "$1" "$2"

# codex.sh
#!/bin/sh
squad-hook send "$1" "$SQUAD_MESSAGE"
```

---

## watch

The daemon writes a message to a file. The agent monitors the file for changes, processes the content, and overwrites the file with its response. The daemon polls for changes and reads new content as the agent's output.

```yaml
agents:
  gemini:
    adapter: watch
    watch_file: .squad/gemini-output.txt
```

`watch_file` is required for watch adapters. The path is relative to the workspace root. The file is created automatically if it does not exist.

### How it works

1. The workflow dispatches a message for `gemini`.
2. The daemon writes the message to `.squad/gemini-output.txt`.
3. The agent (watching the file via `inotify`/`kqueue`) detects the change and reads the content.
4. The agent does its work and overwrites the file with its response.
5. The daemon detects the new content and treats it as the agent's output.

### Setting up a watch agent

Any process that can watch a file works. Example with a shell loop:

```sh
#!/bin/sh
# watch-agent.sh — poll the watched file and respond
while true; do
  content=$(cat .squad/gemini-output.txt)
  if [ -n "$content" ]; then
    response=$(echo "$content" | gemini)
    echo "$response" > .squad/gemini-output.txt
  fi
  sleep 1
done
```

Or use `fswatch`, `inotifywait`, or any file-watching utility.

### Notes

- The watch adapter tracks the last-seen content to avoid re-processing unchanged files.
- `squad init` and `squad start` create the watch file automatically if the agent is configured.

---

## Choosing an Adapter

| Adapter | Best for | Requires |
|---------|----------|----------|
| `mcp` | MCP-capable AI CLIs (Claude Code) | `squad-mcp` in agent MCP config |
| `hook` | Any CLI tool invocable via shell script | Executable script at `hook_script` |
| `watch` | Agents that read/write files | File path at `watch_file` |
