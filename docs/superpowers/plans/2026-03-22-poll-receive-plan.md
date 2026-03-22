# Poll-based Receive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace blocking `--wait` with non-blocking `--poll` as the default AI agent receive pattern, and split the receive transaction as a safety net.

**Architecture:** Add `--poll N` flag to receive command, update slash command templates and role files to use non-blocking receive.

**Tech Stack:** Rust, rusqlite, anyhow

**Spec:** `docs/superpowers/specs/2026-03-22-poll-receive-design.md`

---

## Important: Test Patterns

Follow existing patterns in the codebase:

**`tests/cli_test.rs`:** `TempDir::new()` + `squad(tmp.path())` + `predicate::str::contains` (no 's')

**`tests/e2e_test.rs`:** `setup_workspace()` helper + `squad(tmp.path())`

**`tests/store_test.rs`:** `TempDir::new()` + `Store::open(&tmp.path().join("messages.db"))`

---

### Task 1: Split receive_messages transaction (safety net)

**Files:**
- Modify: `src/store.rs` (`receive_messages`)
- Test: `tests/store_test.rs`

- [ ] **Step 1: Write test for concurrent-safe receive**

Add to `tests/store_test.rs`:

```rust
#[test]
fn test_receive_messages_visible_before_mark_read() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.db");
    let store1 = Store::open(&db_path).unwrap();
    let store2 = Store::open(&db_path).unwrap();

    store1.register_agent("manager", "manager").unwrap();
    store1.register_agent("worker", "worker").unwrap();
    store1.send_message("manager", "worker", "task 1").unwrap();

    // Both connections can see the unread message
    let msgs1 = store1.receive_messages("worker").unwrap();
    let msgs2 = store2.receive_messages("worker").unwrap();

    // At least one should get the message (with split transaction, both might)
    assert!(!msgs1.is_empty() || !msgs2.is_empty());
}
```

- [ ] **Step 2: Run test to see current behavior**

Run: `cargo test test_receive_messages_visible_before_mark_read -- --nocapture`
Expected: Passes (one gets the message). This test documents the behavior.

- [ ] **Step 3: Split the transaction in receive_messages**

In `src/store.rs`, replace the current `receive_messages` (lines 147-173):

```rust
/// Read unread messages, print them, then mark as read.
/// Deliberately NOT atomic — allows multiple consumers to both see the same message.
/// Duplicate delivery is acceptable; message loss is not.
pub fn receive_messages(&self, agent_id: &str) -> Result<Vec<MessageRecord>> {
    // Phase 1: Read unread messages (no transaction — visible to other readers)
    let mut stmt = self.conn.prepare(
        "SELECT id, from_agent, to_agent, content, created_at, read
         FROM messages WHERE to_agent = ?1 AND read = 0 ORDER BY created_at",
    )?;
    let messages: Vec<MessageRecord> = stmt
        .query_map([agent_id], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                from_agent: row.get(1)?,
                to_agent: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                read: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    // Phase 2: Mark as read (separate step — after caller has the data)
    if !messages.is_empty() {
        self.conn.execute(
            "UPDATE messages SET read = 1 WHERE to_agent = ?1 AND read = 0",
            [agent_id],
        )?;
    }
    Ok(messages)
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: all pass. Existing behavior is preserved — the only change is that the SELECT and UPDATE are no longer in a transaction.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs tests/store_test.rs
git commit -m "fix: split receive transaction to prevent message loss with multiple consumers"
```

---

### Task 2: Add --poll flag to receive command

**Files:**
- Modify: `src/main.rs` (receive argument parsing + poll loop)
- Test: `tests/cli_test.rs`

- [ ] **Step 1: Write test for --poll with message**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn test_receive_poll_gets_message() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "manager"]).assert().success();
    squad(tmp.path()).args(["join", "worker"]).assert().success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "task via poll"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["receive", "worker", "--poll"])
        .assert()
        .success()
        .stdout(predicate::str::contains("task via poll"));
}

#[test]
fn test_receive_poll_no_message_returns_empty() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "worker"]).assert().success();

    squad(tmp.path())
        .args(["receive", "worker", "--poll"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No new messages"));
}

#[test]
fn test_receive_poll_with_count() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "worker"]).assert().success();

    // --poll 2 should return quickly when no messages (2 checks × 2s = 4s max)
    squad(tmp.path())
        .args(["receive", "worker", "--poll", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No new messages"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_receive_poll -- --nocapture`
Expected: FAIL — `--poll` flag not recognized.

- [ ] **Step 3: Add --poll flag parsing to main.rs**

In the `"receive"` match arm (main.rs lines 48-72), add `poll` parsing alongside `wait`:

```rust
"receive" => {
    let id = args.next().unwrap_or_default();
    if id.is_empty() {
        bail!("Usage: squad receive <id> [--wait] [--poll [N]] [--timeout <secs>]");
    }
    let mut wait = false;
    let mut poll: Option<u32> = None;
    let mut timeout_secs: u64 = 3600;
    let extra: Vec<String> = args.collect();
    let mut i = 0;
    while i < extra.len() {
        match extra[i].as_str() {
            "--wait" => {
                wait = true;
                i += 1;
            }
            "--poll" => {
                // --poll or --poll N
                if let Some(val) = extra.get(i + 1) {
                    if let Ok(n) = val.parse::<u32>() {
                        poll = Some(n);
                        i += 2;
                        continue;
                    }
                }
                poll = Some(5); // default: 5 checks
                i += 1;
            }
            "--timeout" => {
                if let Some(val) = extra.get(i + 1) {
                    timeout_secs = val.parse().unwrap_or(120);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    cmd_receive(&id, wait, poll, timeout_secs)
}
```

- [ ] **Step 4: Implement poll mode in cmd_receive**

Update `cmd_receive` signature and add poll branch:

```rust
fn cmd_receive(agent: &str, wait: bool, poll: Option<u32>, timeout_secs: u64) -> Result<()> {
    let workspace = find_workspace()?;

    // Validate session at entry
    let store = open_store(&workspace)?;
    check_session(&workspace, &store, agent)?;

    if wait {
        // Existing --wait behavior (unchanged)
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            let store = open_store(&workspace)?;
            check_session(&workspace, &store, agent)?;

            if store.has_unread_messages(agent)? {
                let messages = store.receive_messages(agent)?;
                if !messages.is_empty() {
                    print_messages(&messages, Some(agent));
                    return Ok(());
                }
            }
            if std::time::Instant::now() > deadline {
                println!("No new messages (timed out after {timeout_secs}s).");
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    } else if let Some(count) = poll {
        // --poll N: check N times with 2s interval
        for _ in 0..count {
            let store = open_store(&workspace)?;
            let messages = store.receive_messages(agent)?;
            if !messages.is_empty() {
                print_messages(&messages, Some(agent));
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        println!("No new messages.");
        Ok(())
    } else {
        // Default: check once, return immediately
        let messages = store.receive_messages(agent)?;
        if messages.is_empty() {
            println!("No new messages.");
        } else {
            print_messages(&messages, Some(agent));
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: all pass including 3 new poll tests.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs tests/cli_test.rs
git commit -m "feat: add --poll flag for non-blocking receive (AI-tool safe)"
```

---

### Task 3: Update slash command templates

**Files:**
- Modify: `src/setup.rs` (SQUAD_MD_CONTENT, SQUAD_TOML_CONTENT)

- [ ] **Step 1: Update SQUAD_MD_CONTENT and SQUAD_TOML_CONTENT**

In both templates, make these changes:

**Step 4 (Communicate):** Change `squad receive <your-id> --wait` to `squad receive <your-id>`:
```
4. Communicate using squad commands:
   - `squad send <your-id> <to> "<message>"` — send a message (use @all to broadcast)
   - `squad receive <your-id>` — check for new messages
   - `squad receive <your-id> --poll` — check a few times with short delay
   - `squad agents` — see who is online
   - `squad pending` — check unread messages
   - `squad history` — view message history
```

**Step 5:** Replace blocking wait with non-blocking check:
```
5. After completing any task, check for new messages:
   `squad receive <your-id>`
   If no messages, continue with other work or check again shortly.
```

**Step 6:** Update the agents confirmation step number.

**Step 7 (old IMPORTANT retry):** Remove entirely. No more `--wait` retry loop.

**Step 8 (SESSION CONFLICT):** Renumber to 7. Keep as-is.

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: `test_md_content_has_required_sections` may need updating if it checks for `--wait`.

- [ ] **Step 3: Fix any failing tests in setup_test.rs**

If `test_md_content_has_required_sections` checks for `--wait`, update the assertion.

- [ ] **Step 4: Commit**

```bash
git add src/setup.rs tests/setup_test.rs
git commit -m "feat: update slash command templates to use non-blocking receive"
```

---

### Task 4: Update role templates

**Files:**
- Modify: `src/roles/worker.md`
- Modify: `src/roles/manager.md`
- Modify: `src/roles/inspector.md`

- [ ] **Step 1: Update worker.md**

Replace lines 11-12:
```
- After completing a task, check for new messages with `squad receive <your-id>`
- If no messages, continue with other work or check again shortly.
```

- [ ] **Step 2: Update manager.md**

Replace lines 18-19:
```
- When waiting for results, check for messages with `squad receive manager`
- If no messages yet, continue monitoring or check again shortly.
```

- [ ] **Step 3: Update inspector.md**

Replace lines 19-20:
```
- After completing a review, check for new messages with `squad receive <your-id>`
- If no messages, continue with other work or check again shortly.
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all pass. Role tests check for role existence, not content.

- [ ] **Step 5: Commit**

```bash
git add src/roles/worker.md src/roles/manager.md src/roles/inspector.md
git commit -m "feat: update role templates to use non-blocking receive"
```

---

### Task 5: Update help text and README

**Files:**
- Modify: `src/main.rs` (HELP_TEXT)
- Modify: `README.md`
- Modify: `README.zh-CN.md`

- [ ] **Step 1: Update HELP_TEXT in main.rs**

Change the receive line in COMMANDS section:
```
  squad receive <id> [--poll [N]] [--wait] [--timeout N]  Check inbox (--poll checks N times, default 5)
```

Update HOW TO PARTICIPATE step 4:
```
  4. squad receive <your-id>                Wait for next task or feedback
```

- [ ] **Step 2: Update README.md commands table**

Change:
```
| `squad receive <id> [--wait] [--timeout N]` | Check inbox (`--wait` blocks until message, default 3600s) |
```
To:
```
| `squad receive <id> [--poll [N]] [--wait]` | Check inbox (`--poll` checks N times with 2s interval) |
```

- [ ] **Step 3: Update README.zh-CN.md similarly**

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs README.md README.zh-CN.md
git commit -m "docs: update help text and READMEs for --poll receive"
```

---

### Task 6: Reinstall and verify

- [ ] **Step 1: Run full test suite + clippy**

```bash
cargo test
cargo clippy -- -D warnings
```

- [ ] **Step 2: Reinstall and update slash commands**

```bash
cargo install --path .
squad setup
```

- [ ] **Step 3: Manual smoke test**

```bash
squad clean && squad init
squad join worker --role worker
squad send worker worker "test poll"
squad receive worker --poll     # should return message
squad receive worker --poll     # should return "No new messages"
squad receive worker            # should return "No new messages" (already read)
```

- [ ] **Step 4: Commit version bump**

```bash
# Bump version in Cargo.toml to 0.3.1
git add Cargo.toml
git commit -m "chore: bump version to 0.3.1"
```

---

## Summary

| Task | Files | Tests Added |
|------|-------|-------------|
| 1. Split transaction | store.rs, store_test.rs | 1 |
| 2. --poll flag | main.rs, cli_test.rs | 3 |
| 3. Slash command templates | setup.rs, setup_test.rs | 0 |
| 4. Role templates | worker.md, manager.md, inspector.md | 0 |
| 5. Help + README | main.rs, README.md, README.zh-CN.md | 0 |
| 6. Verify + release | Cargo.toml | 0 |
| **Total** | **10 files** | **4 new tests** |
