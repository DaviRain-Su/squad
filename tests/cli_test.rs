use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

fn squad(workspace: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("squad").unwrap();
    cmd.current_dir(workspace);
    cmd
}

fn mark_agent_stale(workspace: &std::path::Path, agent_id: &str) {
    let db = workspace.join(".squad").join("messages.db");
    let conn = Connection::open(db).unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    conn.execute(
        "UPDATE agents SET last_seen = ?1 WHERE id = ?2",
        rusqlite::params![now - 601, agent_id],
    )
    .unwrap();
}

fn set_message_timestamp(workspace: &std::path::Path, content: &str, created_at: i64) {
    let db = workspace.join(".squad").join("messages.db");
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "UPDATE messages SET created_at = ?1 WHERE content = ?2",
        rusqlite::params![created_at, content],
    )
    .unwrap();
}

#[test]
fn test_init() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));
}

#[test]
fn test_join_and_agents() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Joined as manager"));
    squad(tmp.path())
        .arg("agents")
        .assert()
        .success()
        .stdout(predicate::str::contains("manager"));
}

#[test]
fn test_join_with_role_flag() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "worker-1", "--role", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Joined as worker-1"));
}

#[test]
fn test_send_and_receive() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "implement auth module"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["receive", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[from manager]"))
        .stdout(predicate::str::contains("implement auth module"));
}

#[test]
fn test_send_warns_when_recipient_is_stale() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    mark_agent_stale(tmp.path(), "worker");

    squad(tmp.path())
        .args(["send", "manager", "worker", "implement auth module"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent to worker."))
        .stdout(predicate::str::contains(
            "Warning: worker is stale (last seen 10m ago). Message was queued but may not be seen soon.",
        ));
}

#[test]
fn test_send_reads_message_from_file() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    let message_path = tmp.path().join("message.txt");
    std::fs::write(&message_path, "line 1\nline 2").unwrap();

    squad(tmp.path())
        .args(["send", "--file", "message.txt", "manager", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent to worker."));

    squad(tmp.path())
        .args(["receive", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1\nline 2"));
}

#[test]
fn test_send_reads_message_from_stdin() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "--file", "-", "manager", "worker"])
        .write_stdin("line 1\nline 2")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent to worker."));

    squad(tmp.path())
        .args(["receive", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1\nline 2"));
}

#[test]
fn test_send_broadcast() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker-1"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker-2"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "@all", "interface changed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Broadcast to 2 agents"));

    squad(tmp.path())
        .args(["receive", "worker-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("interface changed"));
}

#[test]
fn test_send_keeps_inline_message_starting_with_file_flag() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "--file", "missing.txt"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["receive", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--file missing.txt"));
}

#[test]
fn test_broadcast_warns_about_stale_recipients() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker-1"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker-2"])
        .assert()
        .success();

    mark_agent_stale(tmp.path(), "worker-2");

    squad(tmp.path())
        .args(["send", "manager", "@all", "interface changed"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Broadcast to 2 agents: worker-1, worker-2",
        ))
        .stdout(predicate::str::contains(
            "Warning: stale recipients: worker-2 (10m ago).",
        ));
}

#[test]
fn test_send_to_nonexistent() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "nobody", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_send_from_nonexistent_fails() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "ghost", "worker", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghost does not exist"));
}

#[test]
fn test_leave() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["leave", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .arg("agents")
        .assert()
        .success()
        .stdout(predicate::str::contains("No agents"));
}

#[test]
fn test_setup_list() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path())
        .args(["setup", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude"))
        .stdout(predicate::str::contains("gemini"))
        .stdout(predicate::str::contains("codex"))
        .stdout(predicate::str::contains("opencode"));
}

#[test]
fn test_help_describes_wait_as_debug_only() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path())
        .arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Check inbox once (`--wait --timeout N` is for manual/debug use)",
        ))
        .stdout(predicate::str::contains("Worker checks once for tasks"));
}

#[test]
fn test_join_freeform_role_succeeds() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "cto"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Joined as cto"))
        .stdout(predicate::str::contains("Interpret this role autonomously"))
        .stdout(predicate::str::contains("squad send"))
        .stdout(predicate::str::contains("squad receive, squad agents"))
        .stdout(predicate::str::contains("--wait").not());
}

#[test]
fn test_history() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "task 1"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["receive", "worker"])
        .assert()
        .success(); // marks as read

    // history still shows it
    squad(tmp.path())
        .arg("history")
        .assert()
        .success()
        .stdout(predicate::str::contains("task 1"));
}

#[test]
fn test_history_shows_timestamps() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "task timestamped"])
        .assert()
        .success();
    set_message_timestamp(tmp.path(), "task timestamped", 1_704_067_200);

    squad(tmp.path())
        .arg("history")
        .assert()
        .success()
        .stdout(predicate::str::contains("[2024-01-01T00:00:00Z]"))
        .stdout(predicate::str::contains("task timestamped"));
}

#[test]
fn test_history_formats_multiline_messages_readably() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "--file", "-", "manager", "worker"])
        .write_stdin("line 1\nline 2")
        .assert()
        .success();
    set_message_timestamp(tmp.path(), "line 1\nline 2", 1_704_067_200);

    squad(tmp.path())
        .arg("history")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "* [2024-01-01T00:00:00Z] manager -> worker: line 1\n  | line 2",
        ));
}

#[test]
fn test_history_filters_by_from_and_to() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    for agent in ["manager", "worker", "inspector"] {
        squad(tmp.path()).args(["join", agent]).assert().success();
    }

    squad(tmp.path())
        .args(["send", "manager", "worker", "task from manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["send", "worker", "manager", "done from worker"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["send", "inspector", "worker", "review from inspector"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["history", "--from", "manager"])
        .assert()
        .success()
        .stdout(predicate::str::contains("task from manager"))
        .stdout(predicate::str::contains("done from worker").not())
        .stdout(predicate::str::contains("review from inspector").not());

    squad(tmp.path())
        .args(["history", "--to", "manager"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done from worker"))
        .stdout(predicate::str::contains("task from manager").not())
        .stdout(predicate::str::contains("review from inspector").not());
}

#[test]
fn test_history_positional_agent_filter_still_works() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    for agent in ["manager", "worker", "inspector"] {
        squad(tmp.path()).args(["join", agent]).assert().success();
    }

    squad(tmp.path())
        .args(["send", "manager", "worker", "task for worker"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["send", "inspector", "manager", "review for manager"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["history", "worker"])
        .assert()
        .success()
        .stdout(predicate::str::contains("task for worker"))
        .stdout(predicate::str::contains("review for manager").not());
}

#[test]
fn test_history_filters_by_since() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "old task"])
        .assert()
        .success();
    set_message_timestamp(tmp.path(), "old task", 1_704_067_200);

    squad(tmp.path())
        .args(["send", "manager", "worker", "new task"])
        .assert()
        .success();
    set_message_timestamp(tmp.path(), "new task", 1_704_067_320);

    squad(tmp.path())
        .args(["history", "--since", "2024-01-01T00:01:00Z"])
        .assert()
        .success()
        .stdout(predicate::str::contains("new task"))
        .stdout(predicate::str::contains("old task").not());
}

#[test]
fn test_history_filters_by_numeric_since() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "manager"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["send", "manager", "worker", "older task"])
        .assert()
        .success();
    set_message_timestamp(tmp.path(), "older task", 1_704_067_200);

    squad(tmp.path())
        .args(["send", "manager", "worker", "latest task"])
        .assert()
        .success();
    set_message_timestamp(tmp.path(), "latest task", 1_704_067_320);

    squad(tmp.path())
        .args(["history", "--since", "1704067260"])
        .assert()
        .success()
        .stdout(predicate::str::contains("latest task"))
        .stdout(predicate::str::contains("older task").not());
}

#[test]
fn test_receive_wait_timeout_suggests_checking_again() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();

    squad(tmp.path())
        .args(["receive", "worker", "--wait", "--timeout", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No new messages (timed out after 0s). Run `squad receive worker` again to keep listening.",
        ));
}

#[test]
fn test_join_creates_session_file() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "worker", "--role", "worker"])
        .assert()
        .success();
    let session_path = tmp.path().join(".squad").join("sessions").join("worker");
    assert!(session_path.exists());
    let token = std::fs::read_to_string(&session_path).unwrap();
    assert_eq!(token.len(), 36); // UUID v4
}

#[test]
fn test_leave_deletes_session_file() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "worker"])
        .assert()
        .success();
    squad(tmp.path())
        .args(["leave", "worker"])
        .assert()
        .success();
    let session_path = tmp.path().join(".squad").join("sessions").join("worker");
    assert!(!session_path.exists());
}
