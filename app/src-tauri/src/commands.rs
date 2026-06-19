use crate::model::{Item, Location, LocationKind};
use crate::{db, importer};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

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

/// Pure import pipeline (no Tauri runtime needed): live-scan the default
/// locations under `home`, then optionally import the tarball.
pub fn import_all(
    conn: &rusqlite::Connection,
    library_root: &Path,
    home: &Path,
    tarball_path: Option<&Path>,
) -> Result<crate::model::ImportSummary, String> {
    let mut summary = crate::model::ImportSummary::default();

    for (label, path, kind) in default_location_candidates(home) {
        let path_str = path.to_string_lossy().to_string();
        let loc_id =
            db::upsert_location(conn, &label, &path_str, kind).map_err(|e| e.to_string())?;
        let scanned = crate::scanner::scan_location(&path, kind).map_err(|e| e.to_string())?;
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
        println!("real import summary: {:?}", summary);
        assert!(
            summary.items_found > 0,
            "expected to find real skills/agents under {home:?}"
        );
        assert!(!db::list_items(&conn).unwrap().is_empty());
    }
}
