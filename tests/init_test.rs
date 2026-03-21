use squad::init::init_workspace;
use tempfile::TempDir;

#[test]
fn test_init_creates_squad_directory() {
    let tmp = TempDir::new().unwrap();
    init_workspace(tmp.path()).unwrap();
    assert!(tmp.path().join(".squad").exists());
    assert!(tmp.path().join(".squad").join("roles").exists());
    assert!(tmp.path().join(".squad").join("teams").exists());
    assert!(tmp.path().join(".squad").join("roles").join("manager.md").exists());
    assert!(tmp.path().join(".squad").join("roles").join("worker.md").exists());
    assert!(tmp.path().join(".squad").join("roles").join("inspector.md").exists());
    assert!(tmp.path().join(".squad").join("teams").join("dev.yaml").exists());
}

#[test]
fn test_init_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    init_workspace(tmp.path()).unwrap();
    init_workspace(tmp.path()).unwrap(); // Should not error
}

#[test]
fn test_init_adds_gitignore() {
    let tmp = TempDir::new().unwrap();
    init_workspace(tmp.path()).unwrap();
    let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(".squad/"));
}

#[test]
fn test_init_does_not_duplicate_gitignore_entry() {
    let tmp = TempDir::new().unwrap();
    init_workspace(tmp.path()).unwrap();
    init_workspace(tmp.path()).unwrap();
    let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    assert_eq!(gitignore.matches(".squad/").count(), 1);
}

#[test]
fn test_init_creates_agent_config_files() {
    let tmp = TempDir::new().unwrap();
    init_workspace(tmp.path()).unwrap();

    for filename in &["CLAUDE.md", "AGENTS.md", "GEMINI.md"] {
        let path = tmp.path().join(filename);
        assert!(path.exists(), "{filename} should exist");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Squad Collaboration"), "{filename} should contain squad section");
        assert!(content.contains("squad join"), "{filename} should contain join command");
        assert!(content.contains("squad receive"), "{filename} should contain receive command");
    }
}

#[test]
fn test_init_does_not_duplicate_agent_config_section() {
    let tmp = TempDir::new().unwrap();
    init_workspace(tmp.path()).unwrap();
    init_workspace(tmp.path()).unwrap();

    let content = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
    assert_eq!(content.matches("## Squad Collaboration").count(), 1);
}
