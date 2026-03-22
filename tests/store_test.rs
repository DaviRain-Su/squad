use squad::store::Store;
use tempfile::TempDir;

#[test]
fn test_store_init_creates_tables() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    let agents = store.list_agents().unwrap();
    assert!(agents.is_empty());
}

#[test]
fn test_register_and_list_agent() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("manager", "manager").unwrap();
    let agents = store.list_agents().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "manager");
}

#[test]
fn test_unregister_agent() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();
    store.register_agent("worker", "worker").unwrap();
    store.unregister_agent("worker").unwrap();
    assert!(store.list_agents().unwrap().is_empty());
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
fn test_has_unread_messages() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("messages.db")).unwrap();

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

    let agents = store.list_agents().unwrap();
    assert_eq!(agents.len(), 2);

    store.send_message("manager", "worker-1", "task A").unwrap();
    store.send_message("manager", "worker-2", "task B").unwrap();

    let msgs1 = store.receive_messages("worker-1").unwrap();
    assert_eq!(msgs1[0].content, "task A");

    let msgs2 = store.receive_messages("worker-2").unwrap();
    assert_eq!(msgs2[0].content, "task B");
}
