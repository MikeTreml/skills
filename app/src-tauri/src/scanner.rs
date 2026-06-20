use crate::hash::hash_path;
use crate::meta::{parse_meta, title_from};
use crate::model::{ItemType, LocationKind, ScannedItem};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn is_md(p: &Path) -> bool {
    p.extension().map_or(false, |e| e.eq_ignore_ascii_case("md"))
}

/// Common repo markdown files that are not items.
fn is_noise(p: &Path) -> bool {
    matches!(
        p.file_name().and_then(|s| s.to_str()).map(str::to_ascii_uppercase).as_deref(),
        Some("README.MD") | Some("LICENSE.MD") | Some("CHANGELOG.MD") | Some("CONTRIBUTING.MD")
    )
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Scan a user-added directory for items of a given type.
///
/// - Agents: every `*.md` file (recursively) is an agent; title from content.
/// - Skills: a folder containing `SKILL.md` is one skill **named after that
///   folder**; any standalone `*.md` not inside such a folder is a single-file
///   skill, titled from its content.
pub fn scan_custom(root: &Path, item_type: ItemType) -> std::io::Result<Vec<ScannedItem>> {
    let mut found = Vec::new();
    if !root.exists() {
        return Ok(found);
    }

    if item_type == ItemType::Agent {
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if entry.file_type().is_file() && is_md(p) && !is_noise(p) {
                let content = std::fs::read_to_string(p).unwrap_or_default();
                found.push(ScannedItem {
                    item_type: ItemType::Agent,
                    name: title_from(&content, &stem(p)),
                    description: parse_meta(&content).description,
                    hash: hash_path(p)?,
                    source_path: p.to_path_buf(),
                });
            }
        }
    } else {
        // Pass 1: folders that contain a SKILL.md — named after the folder.
        let mut claimed: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                let folder = entry.path().parent().unwrap().to_path_buf();
                let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
                let folder_name = folder.file_name().unwrap().to_string_lossy().to_string();
                found.push(ScannedItem {
                    item_type: ItemType::Skill,
                    name: folder_name, // SKILL.md skills are named after their folder
                    description: parse_meta(&content).description,
                    hash: hash_path(&folder)?,
                    source_path: folder.clone(),
                });
                claimed.push(folder);
            }
        }
        // Pass 2: standalone .md not inside a claimed folder — single-file skills.
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !entry.file_type().is_file() || !is_md(p) || is_noise(p) {
                continue;
            }
            if entry.file_name() == "SKILL.md" || claimed.iter().any(|c| p.starts_with(c)) {
                continue;
            }
            let content = std::fs::read_to_string(p).unwrap_or_default();
            found.push(ScannedItem {
                item_type: ItemType::Skill,
                name: title_from(&content, &stem(p)),
                description: parse_meta(&content).description,
                hash: hash_path(p)?,
                source_path: p.to_path_buf(),
            });
        }
    }

    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Find skills (folders with SKILL.md) or agents (top-level *.md) under `root`.
pub fn scan_location(root: &Path, kind: LocationKind) -> std::io::Result<Vec<ScannedItem>> {
    let mut found = Vec::new();
    if !root.exists() {
        return Ok(found);
    }

    if kind.scans_agents() {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let meta = parse_meta(&content);
                let fallback = path.file_stem().unwrap().to_string_lossy().to_string();
                found.push(ScannedItem {
                    item_type: ItemType::Agent,
                    name: non_empty(meta.name, &fallback),
                    description: meta.description,
                    hash: hash_path(&path)?,
                    source_path: path,
                });
            }
        }
    } else {
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                let folder = entry.path().parent().unwrap().to_path_buf();
                let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
                let meta = parse_meta(&content);
                let fallback = folder.file_name().unwrap().to_string_lossy().to_string();
                found.push(ScannedItem {
                    item_type: ItemType::Skill,
                    name: non_empty(meta.name, &fallback),
                    description: meta.description,
                    hash: hash_path(&folder)?,
                    source_path: folder,
                });
            }
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn finds_skills_by_skill_md() {
        let d = tempfile::tempdir().unwrap();
        let skill = d.path().join("nested/babysit");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: babysit\ndescription: watch\n---\n",
        )
        .unwrap();
        // a stray file that is not a skill
        fs::write(d.path().join("README.md"), "hi").unwrap();

        let found = scan_location(d.path(), LocationKind::ClaudeSkills).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "babysit");
        assert_eq!(found[0].item_type, ItemType::Skill);
        assert_eq!(found[0].source_path, skill);
        assert_eq!(found[0].hash.len(), 64);
    }

    #[test]
    fn finds_agents_by_top_level_md() {
        let d = tempfile::tempdir().unwrap();
        fs::write(
            d.path().join("reviewer.md"),
            "---\nname: reviewer\ndescription: reviews code\n---\n",
        )
        .unwrap();
        let found = scan_location(d.path(), LocationKind::Agents).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item_type, ItemType::Agent);
        assert_eq!(found[0].name, "reviewer");
    }

    #[test]
    fn missing_root_returns_empty() {
        let found =
            scan_location(Path::new("/no/such/dir/xyz"), LocationKind::ClaudeSkills).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn name_falls_back_to_folder_when_frontmatter_missing() {
        let d = tempfile::tempdir().unwrap();
        let skill = d.path().join("my-skill");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# no frontmatter\n").unwrap();
        let found = scan_location(d.path(), LocationKind::ClaudeSkills).unwrap();
        assert_eq!(found[0].name, "my-skill");
    }

    #[test]
    fn scan_custom_skill_dir_folder_named_by_parent_plus_loose_md() {
        let d = tempfile::tempdir().unwrap();
        // folder skill: frontmatter name differs from folder; folder name wins
        let folder = d.path().join("real-folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "---\nname: Fancy Name\n---\nbody").unwrap();
        fs::write(folder.join("notes.md"), "# inside claimed folder, not a skill").unwrap();
        // standalone single-file skill, titled by its heading
        fs::write(d.path().join("loose.md"), "# Loose Skill\n\nbody").unwrap();
        // noise file, excluded
        fs::write(d.path().join("README.md"), "# Readme").unwrap();

        let found = scan_custom(d.path(), ItemType::Skill).unwrap();
        let names: Vec<_> = found.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["Loose Skill".to_string(), "real-folder".to_string()]);
        let folder_skill = found.iter().find(|s| s.name == "real-folder").unwrap();
        assert_eq!(folder_skill.source_path, folder);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn scan_custom_agent_dir_titles_from_content() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("a.md"), "---\nname: Alpha Agent\n---\nbody").unwrap();
        fs::write(d.path().join("b.md"), "# Beta Agent\n\nbody").unwrap();
        fs::write(d.path().join("c.md"), "no name, no heading").unwrap();
        fs::write(d.path().join("LICENSE.md"), "# license").unwrap();

        let found = scan_custom(d.path(), ItemType::Agent).unwrap();
        let names: Vec<_> = found.iter().map(|s| s.name.clone()).collect();
        assert_eq!(
            names,
            vec!["Alpha Agent".to_string(), "Beta Agent".to_string(), "c".to_string()]
        );
        assert!(found.iter().all(|s| s.item_type == ItemType::Agent));
    }
}
