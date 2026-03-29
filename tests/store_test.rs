use squad::store::Store;
use tempfile::TempDir;

#[test]
fn test_store_init_creates_tables() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    let agents = store.list_agents(false).unwrap();
    assert!(agents.is_empty());
}

#[test]
fn test_store_open_migrates_legacy_message_schema_without_reset() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE agents (
            id TEXT PRIMARY KEY,
            role TEXT NOT NULL,
            joined_at INTEGER NOT NULL
        );
        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            from_agent TEXT NOT NULL,
            to_agent TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            read INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO agents (id, role, joined_at) VALUES ('worker', 'worker', 100);
        INSERT INTO messages (from_agent, to_agent, content, created_at, read)
        VALUES ('manager', 'worker', 'legacy note', 101, 0);",
    )
    .unwrap();
    drop(conn);

    let store = Store::open(&db_path).unwrap();
    let agents = store.list_agents(false).unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "worker");

    let messages = store.receive_messages("worker").unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "legacy note");
    assert_eq!(messages[0].kind, "note");
    assert_eq!(messages[0].task_id, None);
    assert_eq!(messages[0].reply_to, None);
}

#[test]
fn test_register_and_list_agent() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    let agents = store.list_agents(false).unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "manager");
}

#[test]
fn test_store_open_migrates_agent_capability_columns() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE agents (
            id TEXT PRIMARY KEY,
            role TEXT NOT NULL,
            joined_at INTEGER NOT NULL,
            session_token TEXT,
            last_seen INTEGER,
            status TEXT NOT NULL DEFAULT 'active',
            archived_at INTEGER
        );
        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            from_agent TEXT NOT NULL,
            to_agent TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            read INTEGER NOT NULL DEFAULT 0
        );",
    )
    .unwrap();
    drop(conn);

    let _store = Store::open(&db_path).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let mut stmt = conn.prepare("PRAGMA table_info(agents)").unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert!(columns.contains(&"client_type".to_string()));
    assert!(columns.contains(&"protocol_version".to_string()));
}

#[test]
fn test_unregister_agent() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker", "worker").unwrap();
    store.unregister_agent("worker").unwrap();
    assert!(store.list_agents(false).unwrap().is_empty());
}

#[test]
fn test_unregister_agent_fails_when_update_affects_zero_rows() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker", "worker").unwrap();

    store.unregister_agent("worker").unwrap();

    let result = store.unregister_agent("worker");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("worker is archived"));
}

#[test]
fn test_unregister_archives_agent_and_rejoin_preserves_unread_messages() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    let (joined_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(joined_id, "worker");

    store
        .send_message_checked("manager", "worker", "task-before-leave")
        .unwrap();
    store.unregister_agent("worker").unwrap();

    assert!(!store.agent_exists("worker").unwrap());
    let active_agents = store.list_agents(false).unwrap();
    assert_eq!(active_agents.len(), 1);
    assert_eq!(active_agents[0].id, "manager");

    let (rejoined_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(rejoined_id, "worker");

    let messages = store.receive_messages("worker").unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "task-before-leave");
}

#[test]
fn test_archived_agents_do_not_receive_suffixes_for_new_active_agents() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    let (first_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(first_id, "worker");

    store.unregister_agent("worker").unwrap();

    let (rejoined_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(rejoined_id, "worker");

    let (suffixed_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(suffixed_id, "worker-2");
}

#[test]
fn test_archived_suffix_is_reused_before_allocating_new_suffix() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();

    let (first_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    let (second_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(first_id, "worker");
    assert_eq!(second_id, "worker-2");

    store.unregister_agent("worker-2").unwrap();

    let (reused_id, _) = store.register_agent_unique("worker", "worker").unwrap();
    assert_eq!(reused_id, "worker-2");
}

#[test]
fn test_send_and_receive_messages() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("worker", "worker").unwrap();

    store
        .send_message("manager", "worker", "implement auth module")
        .unwrap();
    store
        .send_message("manager", "worker", "also add tests")
        .unwrap();

    let messages = store.receive_messages("worker").unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].from_agent, "manager");
    assert_eq!(messages[0].content, "implement auth module");

    // Already read — should be empty now
    let again = store.receive_messages("worker").unwrap();
    assert!(again.is_empty());
}

#[test]
fn test_send_to_nonexistent_agent_fails() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();

    let result = store.send_message_checked("manager", "nobody", "hello");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
fn test_send_to_archived_agent_fails() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("worker", "worker").unwrap();
    store.unregister_agent("worker").unwrap();

    let result = store.send_message_checked("manager", "worker", "hello");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("worker is archived"));
}

#[test]
fn test_archived_agent_rejects_heartbeat_and_inbox_checks() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("worker", "worker").unwrap();
    store
        .send_message_checked("manager", "worker", "hello")
        .unwrap();
    store.unregister_agent("worker").unwrap();

    let heartbeat = store.touch_agent("worker");
    assert!(heartbeat.is_err());
    assert!(heartbeat
        .unwrap_err()
        .to_string()
        .contains("worker is archived"));

    let unread = store.has_unread_messages("worker");
    assert!(unread.is_err());
    assert!(unread
        .unwrap_err()
        .to_string()
        .contains("worker is archived"));

    let receive = store.receive_messages("worker");
    assert!(receive.is_err());
    assert!(receive
        .unwrap_err()
        .to_string()
        .contains("worker is archived"));
}

#[test]
fn test_broadcast_message() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("worker-1", "worker").unwrap();
    store.register_agent("worker-2", "worker").unwrap();

    let recipients = store
        .broadcast_message("manager", "code interface changed")
        .unwrap();
    assert_eq!(recipients.len(), 2);
    assert!(recipients.contains(&"worker-1".to_string()));
    assert!(recipients.contains(&"worker-2".to_string()));

    let msgs1 = store.receive_messages("worker-1").unwrap();
    assert_eq!(msgs1.len(), 1);
    assert_eq!(msgs1[0].content, "code interface changed");

    // Manager should NOT receive its own broadcast
    let msgs_mgr = store.receive_messages("manager").unwrap();
    assert!(msgs_mgr.is_empty());
}

#[test]
fn test_broadcast_skips_archived_agents() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("worker-1", "worker").unwrap();
    store.register_agent("worker-2", "worker").unwrap();
    store.unregister_agent("worker-2").unwrap();

    let recipients = store
        .broadcast_message("manager", "code interface changed")
        .unwrap();
    assert_eq!(recipients, vec!["worker-1".to_string()]);

    let msgs1 = store.receive_messages("worker-1").unwrap();
    assert_eq!(msgs1.len(), 1);

    let msgs2 = store.receive_messages("worker-2");
    assert!(msgs2.is_err());
    assert!(msgs2
        .unwrap_err()
        .to_string()
        .contains("worker-2 is archived"));
}

#[test]
fn test_has_unread_messages() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker", "worker").unwrap();

    assert!(!store.has_unread_messages("worker").unwrap());
    store.send_message("manager", "worker", "task").unwrap();
    assert!(store.has_unread_messages("worker").unwrap());

    store.receive_messages("worker").unwrap();
    assert!(!store.has_unread_messages("worker").unwrap());
}

#[test]
fn test_all_messages_history() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker", "worker").unwrap();

    store.send_message("manager", "worker", "task 1").unwrap();
    store.send_message("worker", "manager", "done").unwrap();
    store.receive_messages("worker").unwrap(); // marks task 1 as read

    // all_messages returns read + unread
    let all = store.all_messages(None).unwrap();
    assert_eq!(all.len(), 2);

    // Filtered by agent
    let worker_msgs = store.all_messages(Some("worker")).unwrap();
    assert_eq!(worker_msgs.len(), 2); // both sent-to and sent-from
}

#[test]
fn test_plain_messages_default_to_note_metadata() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker", "worker").unwrap();

    store.send_message("manager", "worker", "task 1").unwrap();

    let messages = store.all_messages(None).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].kind, "note");
    assert_eq!(messages[0].task_id, None);
    assert_eq!(messages[0].reply_to, None);
}

#[test]
fn test_pending_messages() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();

    store.send_message("manager", "worker", "task 1").unwrap();
    store.send_message("inspector", "worker", "review").unwrap();
    assert_eq!(store.pending_messages().unwrap().len(), 2);
}

#[test]
fn test_register_agent_returns_session_token() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    let token = store.register_agent("worker", "worker").unwrap();
    assert!(!token.is_empty());
    assert_eq!(token.len(), 36); // UUID v4 format: 8-4-4-4-12
}

#[test]
fn test_multiple_agents_same_role() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker-1", "worker").unwrap();
    store.register_agent("worker-2", "worker").unwrap();

    let agents = store.list_agents(false).unwrap();
    assert_eq!(agents.len(), 2);

    store.send_message("manager", "worker-1", "task A").unwrap();
    store.send_message("manager", "worker-2", "task B").unwrap();

    let msgs1 = store.receive_messages("worker-1").unwrap();
    assert_eq!(msgs1[0].content, "task A");

    let msgs2 = store.receive_messages("worker-2").unwrap();
    assert_eq!(msgs2[0].content, "task B");
}

#[test]
fn test_archived_agent_pending_task_diagnostics_include_assignee_and_lease_owner_matches_once_per_agent(
) {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("archived-worker", "worker").unwrap();

    let queued_task = store
        .create_task("manager", "archived-worker", "queued", "queued body")
        .unwrap();
    let acked_task = store
        .create_task("manager", "archived-worker", "acked", "acked body")
        .unwrap();
    store.ack_task("archived-worker", &acked_task).unwrap();
    let completed_task = store
        .create_task("manager", "archived-worker", "done", "done body")
        .unwrap();
    store.ack_task("archived-worker", &completed_task).unwrap();
    store
        .complete_task("archived-worker", &completed_task, "done")
        .unwrap();

    store.unregister_agent("archived-worker").unwrap();

    let warnings = store.archived_agents_with_pending_tasks().unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].0, "archived-worker");
    assert_eq!(warnings[0].1, vec![queued_task, acked_task]);
}

#[test]
fn test_archived_agent_pending_task_diagnostics_ignore_active_agents_and_completed_tasks() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    store.register_agent("active-worker", "worker").unwrap();

    let task_id = store
        .create_task("manager", "active-worker", "queued", "queued body")
        .unwrap();

    let warnings = store.archived_agents_with_pending_tasks().unwrap();

    assert!(warnings.is_empty());
    assert_eq!(store.get_task(&task_id).unwrap().unwrap().status, "queued");
}

#[test]
fn test_protocol_diagnostics_use_effective_version_for_active_agents_only() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store
        .register_agent_with_metadata("legacy-null", "worker", Some("codex"), None)
        .unwrap();
    store
        .register_agent_with_metadata("legacy-one", "worker", Some("claude"), Some(1))
        .unwrap();
    store
        .register_agent_with_metadata("modern", "worker", Some("opencode"), Some(2))
        .unwrap();
    store
        .register_agent_with_metadata("archived-legacy", "worker", Some("gemini"), Some(1))
        .unwrap();
    store.unregister_agent("archived-legacy").unwrap();

    let warnings = store.active_agents_below_protocol(2, 1).unwrap();

    assert_eq!(
        warnings,
        vec![
            ("legacy-null".to_string(), 1),
            ("legacy-one".to_string(), 1),
        ]
    );
}
