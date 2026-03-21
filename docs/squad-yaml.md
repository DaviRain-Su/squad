# squad.yaml Reference

Complete reference for `squad.yaml`, the workspace configuration file.

---

## Top-level Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `project` | string | `"my-project"` | Project name shown in TUI and logs |
| `heartbeat_timeout_seconds` | integer | `30` | Seconds before an agent is considered offline |
| `persistence.enabled` | bool | `false` | Enable persistent message storage |
| `agents` | map | `{}` | Per-agent configuration |
| `workflow` | object | see below | Workflow definition |
| `recovery` | object | see below | Agent reconnection policy |

---

## `agents`

A map of agent name → agent configuration. Agents not listed here are assumed to use the `mcp` adapter with default settings.

```yaml
agents:
  <agent-name>:
    adapter: mcp        # mcp | hook | watch
    hook_script: ...    # required when adapter: hook
    watch_file: ...     # required when adapter: watch
```

### Agent fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `adapter` | enum | `mcp` | How the daemon talks to this agent |
| `hook_script` | string | — | Path to shell script (relative to workspace root). Required for `hook` adapter. |
| `watch_file` | string | — | Path to watched file (relative to workspace root). Required for `watch` adapter. |

### Adapter values

| Value | Description |
|-------|-------------|
| `mcp` | Agent connects via `squad-mcp` MCP server |
| `hook` | Daemon calls `hook_script` with `$SQUAD_MESSAGE` |
| `watch` | Daemon writes to `watch_file`; agent overwrites with response |

---

## `workflow`

Defines the workflow execution.

```yaml
workflow:
  mode: loop
  start_at: implement
  max_iterations: 10
  on_timeout: stop
  timeout_seconds: 300
  steps:
    - ...
```

### Workflow fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | enum | `loop` | Execution mode: `loop`, `pipeline`, or `parallel` |
| `start_at` | string | first step id | Step ID to start from |
| `max_iterations` | integer | `10` | Max loop iterations before timeout (loop mode only) |
| `on_timeout` | enum | `stop` | Action when `max_iterations` reached: `stop`, `notify`, `restart` |
| `timeout_seconds` | integer | `300` | Per-step timeout in seconds |
| `steps` | list | `[]` | Ordered list of workflow steps |

`max_iterations` can also be set at the top level as a shorthand:

```yaml
max_iterations: 5   # equivalent to workflow.max_iterations: 5
```

---

## `workflow.steps[]`

Each step defines what one agent should do.

```yaml
steps:
  - id: implement
    agent: builder
    action: implement
    message: "Goal: {goal}\nPrevious: {previous_output}"
    next: review

  - id: review
    agent: reviewer
    action: review
    prompt: "Review iteration {iteration}:\n{previous_output}"
    on_pass: done
    on_fail: implement
    on_timeout: escalate
```

### Step fields

| Field | Aliases | Type | Default | Description |
|-------|---------|------|---------|-------------|
| `id` | — | string | agent name or `step_N` | Unique step identifier |
| `agent` | — | string | — | Agent name (must exist in `agents` or use default mcp) |
| `action` | — | string | `""` | Role label shown in `squad status` |
| `message` | `prompt` | string | `""` | Message sent to the agent. Supports template variables. |
| `next` | `then` | string | — | Next step ID after `mark_done` (default: pipeline next or loop restart) |
| `on_pass` | — | string | — | Next step when `mark_done` summary is a pass |
| `on_fail` | — | string | — | Next step when `mark_done` summary contains `fail` / `changes requested` / `blocked` |
| `on_timeout` | — | string | — | Step to jump to if this step times out |

### Special next values

- `"done"` — ends the workflow immediately.
- Omitting `next` in `pipeline` mode — advances to the next declared step.
- Omitting `next` in `loop` mode — the workflow finishes.

### Template variables

| Variable | Value |
|----------|-------|
| `{goal}` | Goal string from workflow start |
| `{previous_output}` | Summary from the previous `mark_done` call |
| `{iteration}` | Current iteration count (starts at 0) |

### Summary routing

When `on_pass` or `on_fail` is set, the `mark_done` summary is classified:

- Contains `fail`, `changes requested`, or `blocked` (case-insensitive) → `on_fail`
- Anything else → `on_pass`

---

## `recovery`

Controls how the daemon responds to agents going offline.

```yaml
recovery:
  on_agent_offline: reconnect
  reconnect_attempts: 3
  reconnect_interval_seconds: 5
```

### Recovery fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `on_agent_offline` | enum | `reconnect` | Action when agent heartbeat times out |
| `reconnect_attempts` | integer | `3` | Number of reconnect attempts |
| `reconnect_interval_seconds` | integer | `5` | Seconds between reconnect attempts |

### `on_agent_offline` values

| Value | Behavior |
|-------|----------|
| `reconnect` | Attempt to reconnect `reconnect_attempts` times |
| `restart` | Restart the agent (daemon logs the event) |
| `notify` | Log the event and continue |
| `ignore` | Do nothing |

---

## `persistence`

```yaml
persistence:
  enabled: false
```

When `enabled: true`, the daemon persists messages to `.squad/messages.db` (SQLite-backed JSONL store) so they survive daemon restarts.

---

## Complete Example

```yaml
project: my-project

heartbeat_timeout_seconds: 30

persistence:
  enabled: false

recovery:
  on_agent_offline: reconnect
  reconnect_attempts: 3
  reconnect_interval_seconds: 5

agents:
  builder:
    adapter: mcp

  reviewer:
    adapter: hook
    hook_script: .squad/hooks/reviewer.sh

  watcher:
    adapter: watch
    watch_file: .squad/watcher-output.txt

workflow:
  mode: loop
  start_at: implement
  max_iterations: 8
  on_timeout: stop
  timeout_seconds: 300

  steps:
    - id: implement
      agent: builder
      action: implement
      message: |
        Goal: {goal}

        Previous review:
        {previous_output}

        Implement the required changes.
      on_pass: review
      on_fail: implement

    - id: review
      agent: reviewer
      action: review
      message: |
        Review iteration {iteration}.

        Implementation summary:
        {previous_output}

        Respond PASS or FAIL with notes.
      on_pass: done
      on_fail: implement
      on_timeout: done
```
