use squad::setup::{PLATFORMS, SQUAD_MD_CONTENT, SQUAD_TOML_CONTENT, install_command};
use tempfile::TempDir;

#[test]
fn test_platforms_defined() {
    assert!(PLATFORMS.len() >= 4);
    let names: Vec<&str> = PLATFORMS.iter().map(|p| p.name).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"gemini"));
    assert!(names.contains(&"codex"));
    assert!(names.contains(&"opencode"));
}

#[test]
fn test_md_content_has_required_sections() {
    assert!(SQUAD_MD_CONTENT.contains("$ARGUMENTS"));
    assert!(SQUAD_MD_CONTENT.contains("squad join"));
    assert!(SQUAD_MD_CONTENT.contains("squad receive"));
    assert!(SQUAD_MD_CONTENT.contains("squad send"));
    assert!(SQUAD_MD_CONTENT.contains("squad agents"));
}

#[test]
fn test_toml_content_has_required_sections() {
    assert!(SQUAD_TOML_CONTENT.contains("{{args}}"));
    assert!(SQUAD_TOML_CONTENT.contains("squad join"));
    assert!(SQUAD_TOML_CONTENT.contains("squad receive"));
    assert!(SQUAD_TOML_CONTENT.contains("squad send"));
    assert!(SQUAD_TOML_CONTENT.contains("description"));
    assert!(SQUAD_TOML_CONTENT.contains("prompt"));
}

#[test]
fn test_install_command_creates_file() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join("commands");
    let cmd_path = cmd_dir.join("squad.md");

    install_command(&cmd_path, SQUAD_MD_CONTENT).unwrap();

    assert!(cmd_path.exists());
    let content = std::fs::read_to_string(&cmd_path).unwrap();
    assert!(content.contains("squad join"));
}

#[test]
fn test_install_command_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let deep_path = tmp.path().join("a").join("b").join("c").join("squad.md");

    install_command(&deep_path, SQUAD_MD_CONTENT).unwrap();

    assert!(deep_path.exists());
}

#[test]
fn test_install_command_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    let cmd_dir = tmp.path().join("commands");
    std::fs::create_dir_all(&cmd_dir).unwrap();
    let cmd_path = cmd_dir.join("squad.md");
    std::fs::write(&cmd_path, "old content").unwrap();

    install_command(&cmd_path, SQUAD_MD_CONTENT).unwrap();

    let content = std::fs::read_to_string(&cmd_path).unwrap();
    assert!(content.contains("squad join")); // new content, not "old content"
}
