use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn squad(workspace: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("squad").unwrap();
    cmd.current_dir(workspace);
    cmd
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
    squad(tmp.path()).args(["join", "manager"]).assert().success();
    squad(tmp.path()).args(["join", "worker"]).assert().success();

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
fn test_send_broadcast() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "manager"]).assert().success();
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
fn test_send_to_nonexistent() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "manager"]).assert().success();

    squad(tmp.path())
        .args(["send", "manager", "nobody", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_leave() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "manager"]).assert().success();
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
fn test_join_freeform_role_succeeds() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path())
        .args(["join", "cto"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Joined as cto"))
        .stdout(predicate::str::contains("Interpret this role autonomously"))
        .stdout(predicate::str::contains("squad send"));
}

#[test]
fn test_history() {
    let tmp = TempDir::new().unwrap();
    squad(tmp.path()).arg("init").assert().success();
    squad(tmp.path()).args(["join", "manager"]).assert().success();
    squad(tmp.path()).args(["join", "worker"]).assert().success();

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
    squad(tmp.path()).args(["join", "worker"]).assert().success();
    squad(tmp.path()).args(["leave", "worker"]).assert().success();
    let session_path = tmp.path().join(".squad").join("sessions").join("worker");
    assert!(!session_path.exists());
}
