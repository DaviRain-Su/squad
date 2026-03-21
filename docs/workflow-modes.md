# Workflow Modes

squad supports three workflow execution modes configured under `workflow.mode` in `squad.yaml`.

---

## loop

**Default mode.** Steps execute sequentially, cycling back through the workflow until `max_iterations` is reached or a step transitions to `done`.

```yaml
workflow:
  mode: loop
  max_iterations: 10
  on_timeout: stop
  start_at: implement

  steps:
    - id: implement
      agent: builder
      message: "Goal: {goal}\nPrevious: {previous_output}"
      on_pass: done
      on_fail: implement
```

### Behavior

- The workflow starts at `start_at`.
- Each step calls `mark_done` with a summary string.
- The summary is checked for `pass` / `fail` / `blocked` keywords to determine the next step (via `on_pass` / `on_fail`).
- If no `on_pass` / `on_fail` is set, `next` (alias: `then`) is used.
- When `iteration >= max_iterations` and the next step still exists, the `on_timeout` policy fires.

### Timeout policies

| `on_timeout` | Behavior |
|-------------|----------|
| `stop` | Workflow ends. |
| `notify` | Workflow ends (same as stop, daemon logs the event). |
| `restart` | Resets iteration counter and restarts from `start_at`. |

You can also specify a per-step `on_timeout` that names a step ID to jump to when that step times out:

```yaml
steps:
  - id: implement
    agent: builder
    message: "..."
    on_timeout: escalate
```

### Summary routing

The `mark_done` summary is checked for these keywords (case-insensitive):

- `fail`, `changes requested`, `blocked` → routed to `on_fail`
- Anything else → routed to `on_pass`

---

## pipeline

Steps execute sequentially in declaration order. The workflow advances to the next step automatically after each `mark_done`, without looping.

```yaml
workflow:
  mode: pipeline
  start_at: fetch

  steps:
    - id: fetch
      agent: fetcher
      message: "Fetch data for: {goal}"

    - id: transform
      agent: transformer
      message: "Transform the fetched data:\n{previous_output}"

    - id: report
      agent: reporter
      message: "Generate report from:\n{previous_output}"
```

### Behavior

- Steps run in order: `fetch` → `transform` → `report`.
- `next` / `then` can override the default sequential order.
- `on_pass` / `on_fail` routing works the same as in loop mode.
- No iteration counter; the workflow does not repeat.
- When the last step calls `mark_done`, the workflow is complete.

---

## parallel

All steps are dispatched simultaneously at workflow start. The workflow completes when every step has called `mark_done`.

```yaml
workflow:
  mode: parallel

  steps:
    - id: lint
      agent: linter
      message: "Lint the codebase for: {goal}"

    - id: test
      agent: tester
      message: "Run tests for: {goal}"

    - id: security
      agent: scanner
      message: "Security scan for: {goal}"
```

### Behavior

- All agents receive their messages at the same time.
- Each step tracks its own completion independently.
- `iteration` increments once when all steps finish.
- `previous_output` in the final state is all step outputs joined together as `[step_id] summary`.
- Step-level routing (`on_pass`, `on_fail`, `next`) is ignored in parallel mode.

---

## Choosing a Mode

| Use case | Recommended mode |
|----------|-----------------|
| Iterative implement → review cycle | `loop` |
| ETL or multi-stage pipeline | `pipeline` |
| Independent parallel analysis | `parallel` |
| Review gate with retry | `loop` with `on_fail` |

---

## Template Variables

All step `message` (alias: `prompt`) fields support these variables:

| Variable | Description |
|----------|-------------|
| `{goal}` | The goal string passed when the workflow started |
| `{previous_output}` | Summary from the last completed step |
| `{iteration}` | Current iteration number (starts at 0) |
