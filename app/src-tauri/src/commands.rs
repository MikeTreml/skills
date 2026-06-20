use crate::model::{Item, ItemType, Location, LocationKind, ScanDir};
use crate::{ai, db, importer};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;
use walkdir::WalkDir;

/// Build the default set of (label, path, kind) candidates relative to a home dir.
/// Only paths that exist are returned (the tarball is handled separately).
pub fn default_location_candidates(home: &Path) -> Vec<(String, PathBuf, LocationKind)> {
    let mut out = vec![
        (
            "Claude skills".into(),
            home.join(".claude/skills"),
            LocationKind::ClaudeSkills,
        ),
        (
            "Claude agents".into(),
            home.join(".claude/agents"),
            LocationKind::Agents,
        ),
        (
            "Marketplaces".into(),
            home.join(".claude/plugins/marketplaces"),
            LocationKind::Marketplace,
        ),
        (
            "Codex skills".into(),
            home.join(".codex/skills"),
            LocationKind::Codex,
        ),
    ];
    out.retain(|(_, p, _)| p.exists());
    out
}

/// Discover project-level `.claude/agents` and `.claude/skills` directories under
/// `root` (e.g. `~/Repo`), skipping dependency/build/VCS/fixture directories.
/// `.claude/agents` → Agents kind; `.claude/skills` → Project kind.
pub fn discover_project_locations(root: &Path) -> Vec<(String, PathBuf, LocationKind)> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let pruned = |name: &str| {
        matches!(
            name,
            "node_modules" | "target" | ".git" | ".venv" | "dist" | "build"
        ) || name.contains("fixture")
            || name == "_test-run"
    };
    let walker = WalkDir::new(root).into_iter().filter_entry(|e| {
        if e.depth() > 0 && e.file_type().is_dir() {
            if let Some(n) = e.file_name().to_str() {
                return !pruned(n);
            }
        }
        true
    });
    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let p = entry.path();
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let in_dot_claude = p
            .parent()
            .and_then(|pp| pp.file_name())
            .and_then(|s| s.to_str())
            == Some(".claude");
        if !in_dot_claude {
            continue;
        }
        let kind = match name {
            "agents" => LocationKind::Agents,
            "skills" => LocationKind::Project,
            _ => continue,
        };
        let project = p
            .parent()
            .and_then(|pp| pp.parent())
            .and_then(|pj| pj.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_string();
        out.push((format!("{project} ({name})"), p.to_path_buf(), kind));
    }
    out
}

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub library_root: PathBuf,
    pub home: PathBuf,
    pub tarball_path: Option<PathBuf>,
}

#[tauri::command]
pub fn list_items(state: State<AppState>) -> Result<Vec<Item>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_items(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_locations(state: State<AppState>) -> Result<Vec<Location>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_locations(&conn).map_err(|e| e.to_string())
}

/// Resolve the markdown to preview for a library path: the file itself if the
/// path is a single file, otherwise the SKILL.md inside the folder.
fn read_library_content(library_path: &str) -> std::io::Result<String> {
    let p = Path::new(library_path);
    let file = if p.is_file() {
        p.to_path_buf()
    } else {
        p.join("SKILL.md")
    };
    std::fs::read_to_string(file)
}

#[tauri::command]
pub fn get_item_content(state: State<AppState>, id: i64) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let path = db::item_library_path(&conn, id).map_err(|e| e.to_string())?;
    read_library_content(&path).map_err(|e| e.to_string())
}

fn scan_and_import_location(
    conn: &rusqlite::Connection,
    library_root: &Path,
    label: &str,
    path: &Path,
    kind: LocationKind,
    summary: &mut crate::model::ImportSummary,
) -> Result<(), String> {
    let loc_id =
        db::upsert_location(conn, label, &path.to_string_lossy(), kind).map_err(|e| e.to_string())?;
    let scanned = crate::scanner::scan_location(path, kind).map_err(|e| e.to_string())?;
    summary.locations_scanned += 1;
    for item in &scanned {
        importer::import_scanned(conn, library_root, loc_id, path, item, summary)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pure import pipeline (no Tauri runtime needed): scan the default locations
/// under `home`, discover project-level `.claude/{agents,skills}` under `~/Repo`,
/// then optionally import the tarball.
pub fn import_all(
    conn: &rusqlite::Connection,
    library_root: &Path,
    home: &Path,
    tarball_path: Option<&Path>,
) -> Result<crate::model::ImportSummary, String> {
    let mut summary = crate::model::ImportSummary::default();

    let mut locations = default_location_candidates(home);
    locations.extend(discover_project_locations(&home.join("Repo")));
    for (label, path, kind) in locations {
        scan_and_import_location(conn, library_root, &label, &path, kind, &mut summary)?;
    }

    // User-added scan directories, with type-aware custom detection + titling.
    for sd in db::list_scan_dirs(conn).map_err(|e| e.to_string())? {
        if !sd.enabled {
            continue;
        }
        let path = PathBuf::from(&sd.path);
        if !path.exists() {
            continue;
        }
        let label = format!(
            "{} ({})",
            path.file_name().and_then(|s| s.to_str()).unwrap_or("custom"),
            sd.item_type.as_str()
        );
        // Location kind is just for bookkeeping; scan_custom drives detection.
        let kind = if sd.item_type == ItemType::Agent {
            LocationKind::Agents
        } else {
            LocationKind::Project
        };
        let loc_id =
            db::upsert_location(conn, &label, &sd.path, kind).map_err(|e| e.to_string())?;
        let scanned = crate::scanner::scan_custom(&path, sd.item_type).map_err(|e| e.to_string())?;
        summary.locations_scanned += 1;
        for item in &scanned {
            importer::import_scanned(conn, library_root, loc_id, &path, item, &mut summary)
                .map_err(|e| e.to_string())?;
        }
    }

    if let Some(tarball) = tarball_path {
        if tarball.exists() {
            let loc_id = db::upsert_location(
                conn,
                "Inventory tarball",
                &tarball.to_string_lossy(),
                LocationKind::Tarball,
            )
            .map_err(|e| e.to_string())?;
            let staging = library_root.join("_staging");
            importer::import_tarball(conn, library_root, loc_id, tarball, &staging, &mut summary)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(summary)
}

#[tauri::command]
pub fn run_import(state: State<AppState>) -> Result<crate::model::ImportSummary, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    import_all(
        &conn,
        &state.library_root,
        &state.home,
        state.tarball_path.as_deref(),
    )
}

#[derive(serde::Serialize)]
pub struct ClassifySummary {
    pub classified: u32,
    pub total: u32,
}

/// Trim to None if empty (a free fn so the output borrow ties to the input).
fn opt_str(s: &str) -> Option<&str> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

#[tauri::command]
pub fn ai_available() -> bool {
    ai::api_key().is_some()
}

#[tauri::command]
pub async fn classify_all(state: State<'_, AppState>) -> Result<ClassifySummary, String> {
    let api_key = ai::api_key().ok_or("OPENAI_API_KEY is not set")?;
    let todo = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::unclassified_items(&conn).map_err(|e| e.to_string())?
    };
    let total = todo.len() as u32;
    let client = reqwest::Client::new();
    let mut classified = 0u32;
    for chunk in todo.chunks(20) {
        let results =
            ai::classify_batch(&client, &api_key, chunk, crate::taxonomy::CANONICAL_VERBS).await?;
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for (id, c) in &results {
            db::set_classification(
                &conn,
                *id,
                opt_str(&c.object),
                opt_str(&c.sub_object),
                opt_str(&c.verb),
                opt_str(&c.qualifier),
            )
            .map_err(|e| e.to_string())?;
            classified += 1;
        }
    }
    Ok(ClassifySummary { classified, total })
}

#[tauri::command]
pub fn list_scan_dirs(state: State<AppState>) -> Result<Vec<ScanDir>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_scan_dirs(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_scan_dir(state: State<AppState>, path: String, item_type: ItemType) -> Result<(), String> {
    if !Path::new(&path).is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::add_scan_dir(&conn, &path, item_type).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn remove_scan_dir(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::remove_scan_dir(&conn, id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn returns_only_existing_paths() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
        let cands = default_location_candidates(home.path());
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].2, LocationKind::ClaudeSkills);
    }

    #[test]
    fn read_library_content_handles_file_and_folder() {
        let d = tempfile::tempdir().unwrap();
        let folder = d.path().join("skillfolder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "FOLDER BODY").unwrap();
        assert_eq!(
            read_library_content(folder.to_str().unwrap()).unwrap(),
            "FOLDER BODY"
        );
        let f = d.path().join("agent.md");
        fs::write(&f, "FILE BODY").unwrap();
        assert_eq!(read_library_content(f.to_str().unwrap()).unwrap(), "FILE BODY");
    }

    #[test]
    fn import_all_includes_custom_scan_dirs() {
        let home = tempfile::tempdir().unwrap(); // empty: no default/project locations
        let lib = tempfile::tempdir().unwrap();
        let custom = tempfile::tempdir().unwrap();
        // folder skill (frontmatter name should be ignored in favour of folder name)
        let folder = custom.path().join("my-skill-folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "---\nname: Ignored FM\n---\nbody").unwrap();
        // single-file skill titled by its heading
        fs::write(custom.path().join("loose.md"), "# Loose One\nbody").unwrap();

        let conn = db::open_in_memory().unwrap();
        db::add_scan_dir(&conn, custom.path().to_str().unwrap(), ItemType::Skill).unwrap();

        let summary = import_all(&conn, lib.path(), home.path(), None).unwrap();

        let names: Vec<_> = db::list_items(&conn)
            .unwrap()
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert!(names.contains(&"my-skill-folder".to_string()), "got {names:?}");
        assert!(names.contains(&"Loose One".to_string()), "got {names:?}");
        assert!(summary.items_new >= 2);
    }

    #[test]
    fn discovers_project_claude_dirs_and_skips_junk() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("repoA/.claude/agents")).unwrap();
        fs::create_dir_all(root.path().join("repoA/.claude/skills")).unwrap();
        // junk that must be pruned:
        fs::create_dir_all(root.path().join("repoB/node_modules/pkg/.claude/agents")).unwrap();
        fs::create_dir_all(root.path().join("repoC/fixtures/proj/.claude/agents")).unwrap();

        let found = discover_project_locations(root.path());

        assert!(found
            .iter()
            .any(|(l, _, k)| *k == LocationKind::Agents && l.contains("repoA")));
        assert!(found
            .iter()
            .any(|(l, _, k)| *k == LocationKind::Project && l.contains("repoA")));
        assert!(!found
            .iter()
            .any(|(_, p, _)| p.to_string_lossy().contains("node_modules")));
        assert!(!found
            .iter()
            .any(|(_, p, _)| p.to_string_lossy().contains("fixtures")));
        assert_eq!(found.len(), 2);
    }

    /// Opt-in end-to-end check against the real machine. Live-scans this user's
    /// actual skill/agent locations into a throwaway library and asserts that
    /// at least one item was imported. Run with:
    ///   cargo test imports_from_real_machine -- --ignored --nocapture
    #[test]
    #[ignore]
    fn imports_from_real_machine() {
        let home = dirs::home_dir().expect("home dir");
        let lib = tempfile::tempdir().unwrap();
        let conn = db::open_in_memory().unwrap();
        let summary = import_all(&conn, lib.path(), &home, None).unwrap();
        let items = db::list_items(&conn).unwrap();
        let agents = items
            .iter()
            .filter(|i| i.item_type == crate::model::ItemType::Agent)
            .count();
        let skills = items
            .iter()
            .filter(|i| i.item_type == crate::model::ItemType::Skill)
            .count();
        println!("real import summary: {summary:?}");
        println!("unique items: {skills} skills, {agents} agents");
        assert!(
            agents > 2,
            "expected >2 agents after project discovery, got {agents}"
        );
    }
}
