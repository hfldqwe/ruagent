//! Skill management (design §7.2): SKILL.md-standard skills, platform
//! library + project level, distributed to each harness's skill
//! directories. `sync` is idempotent by content hash.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// One discovered skill.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Where it was found: platform | project
    pub source: String,
    pub path: String,
}

/// Where each harness looks for skills (design §7.2 + the survey).
pub fn harness_skill_dirs(root: &Path, harness: &str) -> Vec<PathBuf> {
    match harness {
        "claude-code" => vec![root.join(".claude").join("skills")],
        "opencode" => vec![root.join(".opencode").join("skills")],
        "dsh" => vec![root.join(".agents").join("skills")],
        _ => vec![root.join(".agents").join("skills")],
    }
}

/// Parse the frontmatter of a SKILL.md (name + description).
fn parse_skill_md(dir: &Path) -> Option<(String, String)> {
    let md = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let name = frontmatter_field(&md, "name").unwrap_or_else(|| {
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let description = frontmatter_field(&md, "description").unwrap_or_default();
    Some((name, description))
}

fn frontmatter_field(md: &str, field: &str) -> Option<String> {
    let fm = md.strip_prefix("---\n")?;
    let end = fm.find("\n---")?;
    for line in fm[..end].lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == field {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Discover skills: platform library first, then project (project wins on
/// name collisions — closer scope is more specific).
pub fn discover(platform_dir: &Path, project_dir: Option<&Path>) -> Vec<Skill> {
    let mut skills = Vec::new();
    let mut add_dir = |dir: &Path, source: &str, skills: &mut Vec<Skill>| {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some((name, description)) = parse_skill_md(&path) {
                skills.push(Skill {
                    name,
                    description,
                    source: source.into(),
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
    };
    add_dir(platform_dir, "platform", &mut skills);
    if let Some(project) = project_dir {
        add_dir(project, "project", &mut skills);
    }
    // Project skills override platform skills with the same name.
    let mut seen = std::collections::HashSet::new();
    skills.reverse();
    skills.retain(|s| seen.insert(s.name.clone()));
    skills.reverse();
    skills
}

/// Copy a skill directory into a target directory (idempotent: skips
/// when the SKILL.md content already matches).
pub fn install_skill(skill_dir: &Path, target_base: &Path) -> Result<bool> {
    let name = skill_dir
        .file_name()
        .context("skill dir has no name")?
        .to_string_lossy()
        .into_owned();
    let target = target_base.join(&name);
    let src_md = std::fs::read_to_string(skill_dir.join("SKILL.md"))
        .with_context(|| format!("reading {}", skill_dir.join("SKILL.md").display()))?;
    if let Ok(existing) = std::fs::read_to_string(target.join("SKILL.md")) {
        if existing == src_md {
            return Ok(false); // already in sync
        }
    }
    // Wipe and re-copy for a clean refresh.
    let _ = std::fs::remove_dir_all(&target);
    copy_dir(skill_dir, &target)?;
    Ok(true)
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Sync skills into every enabled harness's skill directories under
/// `project_root`. Returns (installed, skipped).
pub fn sync(skills: &[Skill], project_root: &Path, harnesses: &[String]) -> Result<(usize, usize)> {
    let mut installed = 0;
    let mut skipped = 0;
    for harness in harnesses {
        for dir in harness_skill_dirs(project_root, harness) {
            for skill in skills {
                let skill_dir = PathBuf::from(&skill.path);
                if skill_dir.exists() && install_skill(&skill_dir, &dir)? {
                    installed += 1;
                } else {
                    skipped += 1;
                }
            }
        }
    }
    Ok((installed, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(base: &Path, name: &str, description: &str) {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: \"{description}\"\n---\n\n# {name}\nbody\n"),
        )
        .unwrap();
    }

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-skills-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discover_and_parse() {
        let platform = tmp("plat");
        let project = tmp("proj");
        write_skill(&platform, "deploy", "how to deploy");
        write_skill(&project, "deploy", "project-specific deploy");
        write_skill(&project, "review", "how to review");

        let skills = discover(&platform, Some(&project));
        let deploy = skills.iter().find(|s| s.name == "deploy").unwrap();
        assert_eq!(deploy.source, "project", "project wins on collision");
        assert_eq!(skills.iter().filter(|s| s.name == "deploy").count(), 1);
        assert!(skills.iter().any(|s| s.name == "review"));
    }

    #[test]
    fn sync_installs_and_is_idempotent() {
        let platform = tmp("sync-plat");
        let project_root = tmp("sync-root");
        write_skill(&platform, "deploy", "how to deploy");
        let skills = discover(&platform, None);

        let (installed, _) = sync(&skills, &project_root, &["claude-code".into()]).unwrap();
        assert_eq!(installed, 1);
        let target = project_root
            .join(".claude")
            .join("skills")
            .join("deploy")
            .join("SKILL.md");
        assert!(target.is_file());

        // Second sync: content unchanged -> skipped.
        let (installed2, skipped2) = sync(&skills, &project_root, &["claude-code".into()]).unwrap();
        assert_eq!(installed2, 0);
        assert!(skipped2 > 0);

        // Changed content -> refreshed.
        std::fs::write(
            platform.join("deploy").join("SKILL.md"),
            "---\nname: deploy\ndescription: \"v2\"\n---\n\nnew body\n",
        )
        .unwrap();
        let skills2 = discover(&platform, None);
        let (installed3, _) = sync(&skills2, &project_root, &["claude-code".into()]).unwrap();
        assert_eq!(installed3, 1);
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("v2"));
    }
}
