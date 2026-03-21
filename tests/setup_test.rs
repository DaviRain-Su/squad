use squad::setup::{install_command, PLATFORMS, SQUAD_COMMAND_CONTENT};
use tempfile::TempDir;

#[test]
fn test_platforms_defined() {
    assert!(PLATFORMS.len() >= 3);
    let names: Vec<&str> = PLATFORMS.iter().map(|p| p.name).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"gemini"));
    assert!(names.contains(&"codex"));
}

#[test]
fn test_command_content_has_required_sections() {
    assert!(SQUAD_COMMAND_CONTENT.contains("$ARGUMENTS"));
    assert!(SQUAD_COMMAND_CONTENT.contains("squad join"));
    assert!(SQUAD_COMMAND_CONTENT.contains("squad receive"));
    assert!(SQUAD_COMMAND_CONTENT.contains("squad send"));
    assert!(SQUAD_COMMAND_CONTENT.contains("squad agents"));
}

#[test]
fn test_install_command_creates_file() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join("commands");
    let cmd_path = cmd_dir.join("squad.md");

    install_command(&cmd_path).unwrap();

    assert!(cmd_path.exists());
    let content = std::fs::read_to_string(&cmd_path).unwrap();
    assert!(content.contains("squad join"));
}

#[test]
fn test_install_command_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let deep_path = tmp.path().join("a").join("b").join("c").join("squad.md");

    install_command(&deep_path).unwrap();

    assert!(deep_path.exists());
}

#[test]
fn test_install_command_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join("commands");
    std::fs::create_dir_all(&cmd_dir).unwrap();
    let cmd_path = cmd_dir.join("squad.md");
    std::fs::write(&cmd_path, "old content").unwrap();

    install_command(&cmd_path).unwrap();

    let content = std::fs::read_to_string(&cmd_path).unwrap();
    assert!(content.contains("squad join")); // new content, not "old content"
}
