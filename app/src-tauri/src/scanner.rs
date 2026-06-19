use crate::hash::hash_path;
use crate::meta::parse_meta;
use crate::model::{ItemType, LocationKind, ScannedItem};
use std::path::Path;
use walkdir::WalkDir;

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
}
