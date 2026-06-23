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
    /// Cooperative cancel flag for a running import.
    pub import_cancel: std::sync::atomic::AtomicBool,
    /// Re-entrancy guard: true while an import is in flight.
    pub import_running: std::sync::atomic::AtomicBool,
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

/// The file to read/write for a library path: the file itself, or SKILL.md inside
/// a folder. Inferred from the path SHAPE (a `.md` library_path is a single-file
/// item; anything else is a skill folder) rather than from filesystem existence,
/// so it stays correct even when the target is temporarily missing.
fn library_file(library_path: &str) -> PathBuf {
    let p = Path::new(library_path);
    if p.extension().map_or(false, |e| e.eq_ignore_ascii_case("md")) {
        p.to_path_buf()
    } else {
        p.join("SKILL.md")
    }
}

fn read_library_content(library_path: &str) -> std::io::Result<String> {
    std::fs::read_to_string(library_file(library_path))
}

#[tauri::command]
pub fn get_item_content(state: State<AppState>, id: i64) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let path = db::item_library_path(&conn, id).map_err(|e| e.to_string())?;
    read_library_content(&path).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct RefineResult {
    pub original: String,
    pub proposed: String,
}

#[tauri::command]
pub async fn refine_item(
    state: State<'_, AppState>,
    id: i64,
    directives: Vec<String>,
    tools_add: Vec<String>,
    tools_remove: Vec<String>,
) -> Result<RefineResult, String> {
    let api_key = ai::api_key().ok_or("OPENAI_API_KEY is not set")?;
    let path = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::item_library_path(&conn, id).map_err(|e| e.to_string())?
    };
    let original = read_library_content(&path).map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    let proposed = ai::refine(&client, &api_key, &original, &directives, &tools_add, &tools_remove).await?;
    Ok(RefineResult { original, proposed })
}

#[tauri::command]
pub fn apply_refinement(state: State<AppState>, id: i64, content: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let path = db::item_library_path(&conn, id).map_err(|e| e.to_string())?;
    let file = library_file(&path);
    // Back up the current content (outside the item folder, so it doesn't affect the hash).
    let backups = state.library_root.join("_refine_backups");
    std::fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
    if let Ok(prev) = std::fs::read(&file) {
        let _ = std::fs::write(backups.join(format!("{id}.bak")), prev);
    }
    std::fs::write(&file, &content).map_err(|e| e.to_string())?;
    let new_hash = crate::hash::hash_path(Path::new(&path)).map_err(|e| e.to_string())?;
    db::set_canonical_hash(&conn, id, &new_hash).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct MergeSource {
    pub id: i64,
    pub name: String,
}

#[derive(serde::Serialize)]
pub struct MergeResult {
    pub proposed: String,
    pub sources: Vec<MergeSource>,
}

#[tauri::command]
pub async fn merge_items(state: State<'_, AppState>, ids: Vec<i64>) -> Result<MergeResult, String> {
    let api_key = ai::api_key().ok_or("OPENAI_API_KEY is not set")?;
    let metas: Vec<(i64, String, String)> = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let items = db::list_items(&conn).map_err(|e| e.to_string())?;
        ids.iter()
            .filter_map(|id| {
                items
                    .iter()
                    .find(|i| i.id == *id)
                    .map(|i| (i.id, i.name.clone(), i.library_path.clone()))
            })
            .collect()
    };
    if metas.len() < 2 {
        return Err("Select at least two items to merge.".into());
    }
    let mut pairs = Vec::new();
    let mut sources = Vec::new();
    for (id, name, path) in &metas {
        let content = read_library_content(path).map_err(|e| e.to_string())?;
        pairs.push((name.clone(), content));
        sources.push(MergeSource { id: *id, name: name.clone() });
    }
    let proposed = ai::merge(&reqwest::Client::new(), &api_key, &pairs).await?;
    Ok(MergeResult { proposed, sources })
}

/// Create a brand-new library item from markdown content; returns its id.
fn create_item_from_content(
    conn: &rusqlite::Connection,
    library_root: &Path,
    item_type: ItemType,
    name: &str,
    content: &str,
) -> Result<i64, String> {
    let base_slug = {
        let s = crate::slug::slugify(name);
        if s.is_empty() {
            "item".to_string()
        } else {
            s
        }
    };
    // CRITICAL: never reuse an existing slug. The slug maps 1:1 to a library path,
    // so writing to a taken slug would overwrite another item's library copy (and
    // insert_item_if_absent would return that item's id). Pick a fresh slug first.
    let slug = db::unique_slug(conn, item_type, &base_slug).map_err(|e| e.to_string())?;
    let base = library_root
        .join("_uncategorized")
        .join(item_type.as_str())
        .join(&slug);
    let (file, library_path) = if item_type == ItemType::Agent {
        (base.join(format!("{slug}.md")), base.join(format!("{slug}.md")))
    } else {
        (base.join("SKILL.md"), base.clone())
    };
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, content).map_err(|e| e.to_string())?;
    let hash = crate::hash::hash_path(&library_path).map_err(|e| e.to_string())?;
    let desc = crate::meta::parse_meta(content).description;
    let (id, was_new) = db::insert_item_if_absent(
        conn,
        item_type,
        name,
        &slug,
        &desc,
        &hash,
        &library_path.to_string_lossy(),
    )
    .map_err(|e| e.to_string())?;
    // unique_slug guarantees this; assert so a future regression can't silently
    // return (and then delete) an existing item's id.
    if !was_new {
        return Err("internal error: merged/new item slug was not unique".into());
    }
    Ok(id)
}

fn deleted_backup_dir(library_root: &Path, id: i64) -> PathBuf {
    library_root.join("_deleted_backups").join(id.to_string())
}

/// Move `src` (file or dir) to `dst`, replacing `dst`. Falls back to copy+remove
/// if a plain rename fails (e.g. across volumes).
fn move_path(src: &Path, dst: &Path) -> Result<(), String> {
    if dst.is_dir() {
        std::fs::remove_dir_all(dst).ok();
    } else if dst.is_file() {
        std::fs::remove_file(dst).ok();
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    crate::importer::copy_tree(src, dst).map_err(|e| e.to_string())?;
    if src.is_dir() {
        std::fs::remove_dir_all(src).ok();
    } else {
        std::fs::remove_file(src).ok();
    }
    Ok(())
}

/// Soft-delete (tombstone) an item: move its library copy into `_deleted_backups`
/// (recoverable) and mark the DB record deleted so re-import won't resurrect it.
fn tombstone_item(conn: &rusqlite::Connection, library_root: &Path, id: i64) -> Result<(), String> {
    let lib_path = db::item_library_path(conn, id).map_err(|e| e.to_string())?;
    let src = Path::new(&lib_path);
    if src.exists() {
        let backup = deleted_backup_dir(library_root, id);
        std::fs::remove_dir_all(&backup).ok();
        std::fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
        let base = src.file_name().ok_or("bad library path")?;
        move_path(src, &backup.join(base))?;
    }
    db::set_deleted(conn, id, true).map_err(|e| e.to_string())?;
    Ok(())
}

/// Undo a tombstone: move the library copy back and clear the deleted flag.
fn restore_item(conn: &rusqlite::Connection, library_root: &Path, id: i64) -> Result<(), String> {
    let lib_path = db::item_library_path(conn, id).map_err(|e| e.to_string())?;
    let dst = Path::new(&lib_path);
    let base = dst.file_name().ok_or("bad library path")?;
    let backup = deleted_backup_dir(library_root, id).join(base);
    if backup.exists() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        move_path(&backup, dst)?;
        std::fs::remove_dir_all(deleted_backup_dir(library_root, id)).ok();
    }
    db::set_deleted(conn, id, false).map_err(|e| e.to_string())?;
    Ok(())
}

/// Save a merged file as a NEW library item. mode "replace" archives the sources;
/// mode "delete" tombstones them (recoverable, kept out of re-import).
#[tauri::command]
pub fn save_merge(
    state: State<AppState>,
    ids: Vec<i64>,
    content: String,
    name: String,
    mode: String,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let type_str = db::item_type(&conn, *ids.first().ok_or("no sources")?).map_err(|e| e.to_string())?;
    let item_type = ItemType::parse(&type_str).unwrap_or(ItemType::Skill);
    let id = create_item_from_content(&conn, &state.library_root, item_type, &name, &content)?;
    // Never touch the freshly-created merged item, even if it somehow shares an id.
    let sources = ids.iter().copied().filter(|sid| *sid != id);
    match mode.as_str() {
        "replace" => {
            for sid in sources {
                db::set_archived(&conn, sid, true).map_err(|e| e.to_string())?;
            }
        }
        "delete" => {
            for sid in sources {
                tombstone_item(&conn, &state.library_root, sid)?;
            }
        }
        _ => {}
    }
    Ok(id)
}

/// Save a refinement as a NEW item (keeps the original intact).
#[tauri::command]
pub fn apply_refinement_as_new(
    state: State<AppState>,
    id: i64,
    content: String,
    name: String,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let type_str = db::item_type(&conn, id).map_err(|e| e.to_string())?;
    let item_type = ItemType::parse(&type_str).unwrap_or(ItemType::Skill);
    create_item_from_content(&conn, &state.library_root, item_type, &name, &content)
}

#[tauri::command]
pub fn archive_item(state: State<AppState>, id: i64, archived: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_archived(&conn, id, archived).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_archived(state: State<AppState>) -> Result<Vec<Item>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_archived(&conn).map_err(|e| e.to_string())
}

/// Soft-delete (tombstone) items: library copies move to `_deleted_backups`
/// (recoverable via the Deleted view) and re-import will skip them.
#[tauri::command]
pub fn delete_items(state: State<AppState>, ids: Vec<i64>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    for id in &ids {
        tombstone_item(&conn, &state.library_root, *id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn restore_deleted(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    restore_item(&conn, &state.library_root, id)
}

#[tauri::command]
pub fn list_deleted(state: State<AppState>) -> Result<Vec<Item>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_deleted(&conn).map_err(|e| e.to_string())
}

// ---- Milestone 5: sync & deploy ----

fn placement_abs(root_path: &str, rel_path: &str) -> PathBuf {
    Path::new(root_path).join(rel_path)
}

/// Compare a location target's current content to the library's canonical hash.
fn sync_status(canonical_hash: &str, abs: &Path) -> String {
    if !abs.exists() {
        return "missing".to_string();
    }
    match crate::hash::hash_path(abs) {
        Ok(h) if h == canonical_hash => "in_sync".to_string(),
        Ok(_) => "drifted".to_string(),
        Err(_) => "error".to_string(),
    }
}

/// Replace `dst` (file or dir) with a copy of `src`.
fn copy_over(src: &Path, dst: &Path) -> Result<(), String> {
    if dst.is_dir() {
        std::fs::remove_dir_all(dst).map_err(|e| e.to_string())?;
    } else if dst.exists() {
        std::fs::remove_file(dst).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    importer::copy_tree(src, dst).map_err(|e| e.to_string())
}

/// Back up whatever is at `target` into the library's `_sync_backups` (one slot per placement).
fn backup_before_overwrite(library_root: &Path, placement_id: i64, target: &Path) -> Result<(), String> {
    if !target.exists() {
        return Ok(());
    }
    let dir = library_root.join("_sync_backups").join(placement_id.to_string());
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "backup".to_string());
    importer::copy_tree(target, &dir.join(name)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn item_sync(state: State<AppState>, id: i64) -> Result<Vec<crate::model::PlacementInfo>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let canonical = db::item_canonical_hash(&conn, id).map_err(|e| e.to_string())?;
    let places = db::placements_for_item(&conn, id).map_err(|e| e.to_string())?;
    Ok(places
        .into_iter()
        .map(|(pid, label, root, rel)| {
            let abs = placement_abs(&root, &rel);
            crate::model::PlacementInfo {
                id: pid,
                location_label: label,
                status: sync_status(&canonical, &abs),
                abs_path: abs.to_string_lossy().to_string(),
            }
        })
        .collect())
}

#[tauri::command]
pub fn read_placement(state: State<AppState>, placement_id: i64) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (_item, root, rel) = db::placement_paths(&conn, placement_id).map_err(|e| e.to_string())?;
    read_library_content(&placement_abs(&root, &rel).to_string_lossy()).map_err(|e| e.to_string())
}

/// Push the library copy OUT to the location (location := library); backs up the location first.
#[tauri::command]
pub fn push_to_location(state: State<AppState>, placement_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (item_id, root, rel) = db::placement_paths(&conn, placement_id).map_err(|e| e.to_string())?;
    let lib_path = db::item_library_path(&conn, item_id).map_err(|e| e.to_string())?;
    let abs = placement_abs(&root, &rel);
    backup_before_overwrite(&state.library_root, placement_id, &abs)?;
    copy_over(Path::new(&lib_path), &abs)?;
    let canonical = db::item_canonical_hash(&conn, item_id).map_err(|e| e.to_string())?;
    db::update_placement_sync(&conn, placement_id, &canonical, "in_sync").map_err(|e| e.to_string())
}

/// Pull the location copy INTO the library (library := location); backs up the library first.
#[tauri::command]
pub fn pull_from_location(state: State<AppState>, placement_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (item_id, root, rel) = db::placement_paths(&conn, placement_id).map_err(|e| e.to_string())?;
    let abs = placement_abs(&root, &rel);
    if !abs.exists() {
        return Err("location copy is missing".into());
    }
    let lib_path = db::item_library_path(&conn, item_id).map_err(|e| e.to_string())?;
    backup_before_overwrite(&state.library_root, placement_id, Path::new(&lib_path))?;
    copy_over(&abs, Path::new(&lib_path))?;
    let new_hash = crate::hash::hash_path(Path::new(&lib_path)).map_err(|e| e.to_string())?;
    db::set_canonical_hash(&conn, item_id, &new_hash).map_err(|e| e.to_string())?;
    db::update_placement_sync(&conn, placement_id, &new_hash, "in_sync").map_err(|e| e.to_string())
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
    report: &dyn Fn(String),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<crate::model::ImportSummary, String> {
    let mut summary = crate::model::ImportSummary::default();

    let mut locations = default_location_candidates(home);
    locations.extend(discover_project_locations(&home.join("Repo")));
    for (label, path, kind) in locations {
        if is_cancelled() {
            summary.cancelled = true;
            return Ok(summary);
        }
        report(format!("Scanning {label}…"));
        scan_and_import_location(conn, library_root, &label, &path, kind, &mut summary)?;
    }

    // User-added scan directories, with type-aware custom detection + titling.
    for sd in db::list_scan_dirs(conn).map_err(|e| e.to_string())? {
        if is_cancelled() {
            summary.cancelled = true;
            return Ok(summary);
        }
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
            importer::import_tarball(
                conn,
                library_root,
                loc_id,
                tarball,
                &staging,
                &mut summary,
                report,
                is_cancelled,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(summary)
}

/// Run the import on a blocking thread so the UI/event loop stays free and a
/// concurrent `cancel_import` can be honored. The `MutexGuard<Connection>` is
/// created and dropped entirely inside the synchronous `spawn_blocking` closure
/// (no `.await` inside it), so the connection never crosses an await point.
#[tauri::command]
pub async fn run_import(app: tauri::AppHandle) -> Result<crate::model::ImportSummary, String> {
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};

    // Reject a second import while one is already running; reset the cancel flag.
    {
        let state = app.state::<AppState>();
        if state
            .import_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("An import is already running.".into());
        }
        state.import_cancel.store(false, Ordering::SeqCst);
    }

    let app2 = app.clone();
    let join = tauri::async_runtime::spawn_blocking(move || {
        // Re-fetch managed state ON this blocking thread (AppState is
        // Send + Sync + 'static — the only bound state::<T>() requires — so no
        // Arc<Mutex<…>> is needed; the `db` field stays a plain Mutex).
        let state = app2.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let report = |msg: String| {
            let _ = app2.emit("import-progress", msg);
        };
        let is_cancelled = || state.import_cancel.load(Ordering::SeqCst);
        import_all(
            &conn,
            &state.library_root,
            &state.home,
            state.tarball_path.as_deref(),
            &report,
            &is_cancelled,
        )
        // `conn` (the guard) drops here, at the end of the closure.
    });

    // JoinHandle's Output is tauri::Result<T>; map the JoinError (e.g. a panic in
    // the blocking task) to a String. Whether the run was cancelled now travels in
    // ImportSummary.cancelled (set by import_all), so the awaited return value alone
    // tells the UI what happened — no terminal event, no event/return-order race.
    let result = join.await.map_err(|e| e.to_string());

    // Always clear the running flag (success, cancel, error, or panic).
    app.state::<AppState>()
        .import_running
        .store(false, Ordering::SeqCst);

    match result {
        Ok(inner) => inner,
        Err(join_err) => Err(join_err),
    }
}

/// Request cancellation of a running import. Flips an atomic only — never touches
/// the DB lock — so it returns instantly even while the import holds the connection.
#[tauri::command]
pub fn cancel_import(state: State<AppState>) {
    state
        .import_cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
}

#[derive(serde::Serialize)]
pub struct ClassifySummary {
    pub classified: u32,
    pub total: u32,
}

#[derive(serde::Serialize, Clone)]
pub struct ClassifyProgress {
    pub done: u32,
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

/// Classify items. `ids = None` → all unclassified; `Some(list)` → exactly those.
/// Emits a `classify-progress` event after each batch.
#[tauri::command]
pub async fn classify_all(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    ids: Option<Vec<i64>>,
) -> Result<ClassifySummary, String> {
    use tauri::Emitter;
    let api_key = ai::api_key().ok_or("OPENAI_API_KEY is not set")?;
    // Scope the guard to this block so it is dropped before the await loop (not Send).
    let (todo, verb_map): (Vec<(i64, String, String)>, std::collections::HashMap<String, String>) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let todo = match &ids {
            Some(list) => {
                let items = db::list_items(&conn).map_err(|e| e.to_string())?;
                list.iter()
                    .filter_map(|id| {
                        items
                            .iter()
                            .find(|i| i.id == *id)
                            .map(|i| (i.id, i.name.clone(), i.description.clone()))
                    })
                    .collect()
            }
            None => db::unclassified_items(&conn).map_err(|e| e.to_string())?,
        };
        // The editable verb map overrides the model's verb (so verb-map edits take effect).
        let verb_map = db::verb_lookup(&conn).map_err(|e| e.to_string())?;
        (todo, verb_map)
    };
    let total = todo.len() as u32;
    let client = reqwest::Client::new();
    let mut classified = 0u32;
    for chunk in todo.chunks(20) {
        let results =
            ai::classify_batch(&client, &api_key, chunk, crate::taxonomy::CANONICAL_VERBS).await?;
        {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            for (id, c) in &results {
                let verb = verb_map
                    .get(&c.verb.to_ascii_lowercase())
                    .cloned()
                    .unwrap_or_else(|| c.verb.clone());
                db::set_classification(
                    &conn,
                    *id,
                    opt_str(&c.object),
                    opt_str(&c.sub_object),
                    opt_str(&verb),
                    opt_str(&c.qualifier),
                )
                .map_err(|e| e.to_string())?;
                classified += 1;
            }
        }
        let _ = app.emit("classify-progress", ClassifyProgress { done: classified, total });
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

#[tauri::command]
pub fn list_duplicates(state: State<AppState>) -> Result<Vec<crate::dedup::DupGroup>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let items = db::list_items(&conn).map_err(|e| e.to_string())?;
    Ok(crate::dedup::group_duplicates(&items))
}

#[tauri::command]
pub fn list_verb_map(state: State<AppState>) -> Result<Vec<(String, String)>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_verb_map(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_synonym(state: State<AppState>, canonical: String, synonym: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::add_synonym(&conn, &canonical, &synonym).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_synonym(state: State<AppState>, synonym: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::remove_synonym(&conn, &synonym).map_err(|e| e.to_string())
}

/// Re-map every classified item's verb through the current verb map; returns how many changed.
#[tauri::command]
pub fn renormalize_verbs(state: State<AppState>) -> Result<u32, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let map = db::verb_lookup(&conn).map_err(|e| e.to_string())?;
    let mut changed = 0u32;
    for it in db::list_items(&conn).map_err(|e| e.to_string())? {
        if let Some(v) = &it.verb {
            if let Some(canon) = map.get(&v.to_ascii_lowercase()) {
                if canon != v {
                    db::set_verb(&conn, it.id, canon).map_err(|e| e.to_string())?;
                    changed += 1;
                }
            }
        }
    }
    Ok(changed)
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
    fn sync_status_detects_states() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.md");
        fs::write(&f, "hello").unwrap();
        let h = crate::hash::hash_path(&f).unwrap();
        assert_eq!(sync_status(&h, &f), "in_sync");
        fs::write(&f, "changed").unwrap();
        assert_eq!(sync_status(&h, &f), "drifted");
        assert_eq!(sync_status(&h, &d.path().join("missing.md")), "missing");
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

        let summary = import_all(&conn, lib.path(), home.path(), None, &|_| {}, &|| false).unwrap();

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
    fn import_all_short_circuits_when_cancelled() {
        let home = tempfile::tempdir().unwrap(); // empty: no default/project locations
        let lib = tempfile::tempdir().unwrap();
        let custom = tempfile::tempdir().unwrap();
        let folder = custom.path().join("would-import");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "---\nname: x\n---\nbody").unwrap();

        let conn = db::open_in_memory().unwrap();
        db::add_scan_dir(&conn, custom.path().to_str().unwrap(), ItemType::Skill).unwrap();

        // Already cancelled → returns Ok with an empty, valid catalog.
        let summary = import_all(&conn, lib.path(), home.path(), None, &|_| {}, &|| true).unwrap();

        assert_eq!(summary.items_new, 0);
        assert!(summary.cancelled, "summary flags the cancellation");
        assert_eq!(db::list_items(&conn).unwrap().len(), 0);
    }

    #[test]
    fn tombstone_then_restore_round_trip() {
        let lib = tempfile::tempdir().unwrap();
        let conn = db::open_in_memory().unwrap();
        let folder = lib.path().join("_uncategorized/skill/foo");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "body").unwrap();
        let (id, _) = db::insert_item_if_absent(
            &conn,
            ItemType::Skill,
            "foo",
            "foo",
            "d",
            "h",
            folder.to_str().unwrap(),
        )
        .unwrap();

        tombstone_item(&conn, lib.path(), id).unwrap();
        assert!(!folder.exists(), "library copy moved into _deleted_backups");
        assert!(db::list_items(&conn).unwrap().is_empty(), "hidden from library");
        assert_eq!(db::list_deleted(&conn).unwrap().len(), 1, "shown in Deleted");

        restore_item(&conn, lib.path(), id).unwrap();
        assert!(folder.join("SKILL.md").exists(), "library copy restored");
        assert_eq!(db::list_items(&conn).unwrap().len(), 1, "back in library");
        assert!(db::list_deleted(&conn).unwrap().is_empty());
    }

    #[test]
    fn create_item_never_overwrites_a_colliding_slug() {
        let lib = tempfile::tempdir().unwrap();
        let conn = db::open_in_memory().unwrap();
        // An existing source "foo" with real library content.
        let foo_dir = lib.path().join("_uncategorized/skill/foo");
        fs::create_dir_all(&foo_dir).unwrap();
        fs::write(foo_dir.join("SKILL.md"), "ORIGINAL FOO").unwrap();
        let (foo_id, _) = db::insert_item_if_absent(
            &conn,
            ItemType::Skill,
            "foo",
            "foo",
            "d",
            "h",
            foo_dir.to_str().unwrap(),
        )
        .unwrap();

        // Create a new item NAMED "Foo" — slugifies to the taken "foo".
        let new_id =
            create_item_from_content(&conn, lib.path(), ItemType::Skill, "Foo", "MERGED").unwrap();

        assert_ne!(new_id, foo_id, "must be a brand-new item, not the source's id");
        assert_eq!(
            fs::read_to_string(foo_dir.join("SKILL.md")).unwrap(),
            "ORIGINAL FOO",
            "the source's library copy must be untouched"
        );
        let merged_path = db::item_library_path(&conn, new_id).unwrap();
        assert!(merged_path.contains("foo-2"), "fresh slug, got {merged_path}");
        assert_eq!(db::list_items(&conn).unwrap().len(), 2);
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
        let summary = import_all(&conn, lib.path(), &home, None, &|_| {}, &|| false).unwrap();
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
