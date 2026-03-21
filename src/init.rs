use anyhow::{Context, Result};
use std::path::Path;

use crate::roles;
use crate::teams;

pub fn init_workspace(workspace: &Path) -> Result<()> {
    let squad_dir = workspace.join(".squad");
    let roles_dir = squad_dir.join("roles");
    let teams_dir = squad_dir.join("teams");

    std::fs::create_dir_all(&roles_dir)
        .with_context(|| format!("failed to create {}", roles_dir.display()))?;
    std::fs::create_dir_all(&teams_dir)
        .with_context(|| format!("failed to create {}", teams_dir.display()))?;

    // Write builtin role templates (skip if already exist)
    for role in roles::BUILTIN_ROLES {
        let path = roles_dir.join(format!("{role}.md"));
        if !path.exists() {
            if let Some(content) = roles::default_role_prompt(role) {
                std::fs::write(&path, content)
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
        }
    }

    // Write builtin team templates (skip if already exist)
    for team_name in teams::BUILTIN_TEAMS {
        let path = teams_dir.join(format!("{team_name}.yaml"));
        if !path.exists() {
            if let Some(team) = teams::default_team(team_name) {
                let content = serde_yaml::to_string(&team)
                    .with_context(|| format!("failed to serialize team: {team_name}"))?;
                std::fs::write(&path, content)
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
        }
    }

    // Add .squad/ to .gitignore
    let gitignore_path = workspace.join(".gitignore");
    let entry = ".squad/";
    let needs_add = if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        !content.lines().any(|line| line.trim() == entry)
    } else {
        true
    };
    if needs_add {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)?;
        writeln!(file, "{entry}")?;
    }

    Ok(())
}
