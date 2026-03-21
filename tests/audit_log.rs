use anyhow::Result;
use squad::daemon::audit::{AuditEventKind, AuditFilter, AuditLog};
use tempfile::tempdir;

#[test]
fn audit_log_round_trips_and_filters_entries() -> Result<()> {
    let workspace = tempdir()?;
    let path = workspace.path().join("audit.log");
    let mut audit = AuditLog::new(&path);

    audit.append(
        "session-1",
        AuditEventKind::AgentRegistered,
        Some("cc"),
        "registered implementer",
    )?;
    audit.append(
        "session-1",
        AuditEventKind::MessageSent,
        Some("cc"),
        "assistant -> cc",
    )?;
    audit.append(
        "session-2",
        AuditEventKind::AgentOffline,
        Some("codex"),
        "codex disconnected",
    )?;

    let all = audit.read_entries(None, None)?;
    assert_eq!(all.len(), 3);

    let filtered = audit.read_entries(Some(2), Some(&AuditFilter::parse("agent=cc")?))?;
    assert_eq!(filtered.len(), 2);
    assert!(filtered
        .iter()
        .all(|entry| entry.agent.as_deref() == Some("cc")));

    let rendered = audit.render(Some(2), Some(&AuditFilter::parse("agent=cc")?))?;
    assert!(rendered.contains("MessageSent"));
    assert!(rendered.contains("AgentRegistered"));
    Ok(())
}
