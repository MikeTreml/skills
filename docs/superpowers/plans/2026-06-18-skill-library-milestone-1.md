# Skill & Agent Library — Milestone 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working Tauri desktop app that scans this machine's skill/agent locations, imports the existing skills tarball, stores canonical copies + metadata in SQLite, and lists everything in a minimal UI.

**Architecture:** Tauri 2 desktop app. Rust core does all filesystem work (scan, hash, copy) and owns a SQLite database via `rusqlite`, exposed to the web frontend as `#[tauri::command]`s over a shared `tauri::State`. The frontend (Vanilla TypeScript + Vite, the Tauri default) calls `invoke()` and renders a flat item list — the tree/browse UI is Milestone 2.

**Tech Stack:** Tauri 2, Rust (rusqlite+bundled SQLite, walkdir, sha2, tar, flate2, serde), TypeScript + Vite.

---

## Roadmap (this plan = Milestone 1 only)

1. **Scaffold + scan + import** ← _this plan_ — app shell, SQLite, scanner, tarball seed + live scan, populate the catalog; minimal list UI.
2. Tree + browse + item page (read-only UI).
3. AI classify (Haiku batch tagging, editable taxonomy, Untriaged bucket).
4. Sync engine (drift detection, per-location diff, push/pull with backups + dry-run).
5. Merge engine (variant reconcile + hand-picked fuse, AI merge→refactor→diff→save).

Spec: `docs/superpowers/specs/2026-06-18-skill-library-app-design.md`. Each later milestone gets its own plan.

## File structure (created in this milestone)

All under `app/` (the Tauri project) in the repo.

| File | Responsibility |
|---|---|
| `app/src-tauri/src/main.rs` | Thin entry; calls `lib::run()`. |
| `app/src-tauri/src/lib.rs` | Build Tauri app, open DB, build `AppState`, register commands. |
| `app/src-tauri/src/model.rs` | Shared structs: `ItemType`, `LocationKind`, `Item`, `Location`, `ScannedItem`, `ImportSummary`. |
| `app/src-tauri/src/slug.rs` | `slugify(name) -> String`. |
| `app/src-tauri/src/meta.rs` | Parse `name`/`description` from SKILL.md / agent frontmatter. |
| `app/src-tauri/src/hash.rs` | `hash_path(path) -> String` deterministic hash of a file or folder. |
| `app/src-tauri/src/db.rs` | Open connection, init schema, upsert/query items/locations/placements. |
| `app/src-tauri/src/scanner.rs` | Walk a location root by kind → `Vec<ScannedItem>`. |
| `app/src-tauri/src/importer.rs` | Default locations, live scan, tarball extract → DB + library copies. |
| `app/src-tauri/src/commands.rs` | Tauri command wrappers calling db/importer. |
| `app/src/api.ts` | Typed `invoke` wrappers + TS types mirroring the Rust structs. |
| `app/src/main.ts` | Wire the "Import" button + render the item list. |
| `app/index.html` | Minimal markup (button + list container). |

The library (canonical copies) lives at the OS app-data dir, `…/skill-library/library/`; the DB at `…/skill-library/catalog.db`. Both are created on first run.

---

## Task 1: Prerequisites & scaffold the Tauri app

**Files:** creates `app/` (scaffold output).

- [ ] **Step 1: Verify toolchain**

Run each; all must succeed:
```bash
rustc --version      # expect 1.77+  (rustup install if missing)
node --version       # expect 18+
npm --version
```
Windows also needs **Microsoft C++ Build Tools** and **WebView2** (bundled on Win11). If `cargo build` later fails linking, install "Desktop development with C++" from the VS Build Tools installer.

- [ ] **Step 2: Scaffold the app**

From the repo root (`C:\Users\miket\Repo\skills`):
```bash
npm create tauri-app@latest app -- --template vanilla-ts --manager npm --yes
```
If the `--` flags are rejected by your npm version, run `npm create tauri-app@latest` interactively and choose: project name `app`, **TypeScript**, **Vanilla**, package manager **npm**.

- [ ] **Step 3: Install JS deps and confirm it builds**

```bash
cd app && npm install && npm run tauri build -- --debug
```
Expected: a debug build completes (first build is slow — it compiles Rust). If WebView2/C++ errors appear, fix per Step 1.

- [ ] **Step 4: Confirm the dev app launches**

Run: `cd app && npm run tauri dev`
Expected: a desktop window opens showing the default Tauri template. Close it.

- [ ] **Step 5: Commit**

```bash
git add app .gitignore
git commit -m "chore: scaffold Tauri 2 app (vanilla-ts)"
```
(If the scaffold added its own `.gitignore` under `app/`, keep it — it ignores `node_modules` and `target`.)

---

## Task 2: Add Rust dependencies

**Files:** Modify `app/src-tauri/Cargo.toml`.

- [ ] **Step 1: Add the crates**

In `app/src-tauri/Cargo.toml`, under `[dependencies]` (tauri/serde/serde_json are already present from the scaffold), add:
```toml
rusqlite = { version = "0.32", features = ["bundled"] }
walkdir = "2"
sha2 = "0.10"
tar = "0.4"
flate2 = "1"
dirs = "5"
```
And add a dev-dependencies section:
```toml
[dev-dependencies]
tempfile = "3"
```
(If a crate's exact version no longer resolves, accept the nearest newer compatible version `cargo add` selects.)

- [ ] **Step 2: Verify it resolves**

Run: `cd app/src-tauri && cargo build`
Expected: dependencies download and compile; build succeeds.

- [ ] **Step 3: Commit**

```bash
git add app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock
git commit -m "chore: add rusqlite, walkdir, sha2, tar, flate2 deps"
```

---

## Task 3: Domain model

**Files:** Create `app/src-tauri/src/model.rs`; Modify `app/src-tauri/src/lib.rs` (add `mod model;`).

- [ ] **Step 1: Write the model**

Create `app/src-tauri/src/model.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Skill,
    Agent,
}

impl ItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Skill => "skill",
            ItemType::Agent => "agent",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(ItemType::Skill),
            "agent" => Some(ItemType::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocationKind {
    ClaudeSkills,
    Marketplace,
    Agents,
    Project,
    Codex,
    Tarball,
}

impl LocationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LocationKind::ClaudeSkills => "claude-skills",
            LocationKind::Marketplace => "marketplace",
            LocationKind::Agents => "agents",
            LocationKind::Project => "project",
            LocationKind::Codex => "codex",
            LocationKind::Tarball => "tarball",
        }
    }
    /// What the scanner looks for: agents = top-level `*.md`, everything else = `**/SKILL.md`.
    pub fn scans_agents(self) -> bool {
        matches!(self, LocationKind::Agents)
    }
}

/// A skill/agent discovered on disk, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedItem {
    pub item_type: ItemType,
    pub name: String,
    pub description: String,
    pub source_path: std::path::PathBuf, // the item's folder (skill) or file (agent)
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub item_type: ItemType,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub category: Option<String>,
    pub subcategory: Option<String>,
    pub canonical_hash: String,
    pub library_path: String,
    pub has_variants: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: i64,
    pub label: String,
    pub root_path: String,
    pub kind: LocationKind,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportSummary {
    pub locations_scanned: u32,
    pub items_found: u32,
    pub items_new: u32,
    pub placements_recorded: u32,
    pub variants_flagged: u32,
}
```

- [ ] **Step 2: Register the module**

In `app/src-tauri/src/lib.rs`, add near the top (below any existing `mod` lines):
```rust
mod model;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd app/src-tauri && cargo build`
Expected: success (warnings about unused code are fine for now).

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri/src/model.rs app/src-tauri/src/lib.rs
git commit -m "feat: add domain model (Item, Location, ScannedItem)"
```

---

## Task 4: Slugify (TDD)

**Files:** Create `app/src-tauri/src/slug.rs`; Modify `lib.rs` (`mod slug;`).

- [ ] **Step 1: Write the failing test**

Create `app/src-tauri/src/slug.rs`:
```rust
pub fn slugify(name: &str) -> String {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_dashes() {
        assert_eq!(slugify("Systematic Debugging"), "systematic-debugging");
    }

    #[test]
    fn collapses_non_alnum_and_trims() {
        assert_eq!(slugify("A/B  Test_Design!"), "a-b-test-design");
        assert_eq!(slugify("  --Edge--  "), "edge");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(slugify("   "), "");
    }
}
```
Add `mod slug;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/src-tauri && cargo test slug::`
Expected: panics with `not implemented`.

- [ ] **Step 3: Implement**

Replace the `slugify` body:
```rust
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true; // true so leading separators are dropped
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test slug::`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/slug.rs app/src-tauri/src/lib.rs
git commit -m "feat: add slugify with tests"
```

---

## Task 5: Frontmatter metadata parser (TDD)

**Files:** Create `app/src-tauri/src/meta.rs`; Modify `lib.rs` (`mod meta;`).

- [ ] **Step 1: Write the failing test**

Create `app/src-tauri/src/meta.rs`:
```rust
/// Extracted front-matter fields. Missing fields become empty strings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    pub name: String,
    pub description: String,
}

/// Parse the leading `---` YAML front matter for `name:` and `description:`.
/// Only simple single-line scalar values are supported (sufficient for SKILL.md).
pub fn parse_meta(content: &str) -> Meta {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_and_description() {
        let c = "---\nname: babysit\ndescription: Watch a task and report changes\n---\n# Body\n";
        assert_eq!(
            parse_meta(c),
            Meta { name: "babysit".into(), description: "Watch a task and report changes".into() }
        );
    }

    #[test]
    fn strips_surrounding_quotes() {
        let c = "---\nname: \"a-b\"\ndescription: 'has, comma'\n---\n";
        assert_eq!(parse_meta(c).description, "has, comma");
        assert_eq!(parse_meta(c).name, "a-b");
    }

    #[test]
    fn no_frontmatter_is_empty() {
        assert_eq!(parse_meta("# Just a heading\n"), Meta::default());
    }
}
```
Add `mod meta;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/src-tauri && cargo test meta::`
Expected: panics with `not implemented`.

- [ ] **Step 3: Implement**

Replace the `parse_meta` body:
```rust
pub fn parse_meta(content: &str) -> Meta {
    let mut meta = Meta::default();
    let trimmed = content.trim_start_matches('\u{feff}');
    let mut lines = trimmed.lines();
    if lines.next().map(str::trim) != Some("---") {
        return meta;
    }
    for line in lines {
        let line = line.trim_end();
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = unquote(value.trim());
            match key {
                "name" => meta.name = value,
                "description" => meta.description = value,
                _ => {}
            }
        }
    }
    meta
}

fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if s.len() >= 2
        && ((bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test meta::`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/meta.rs app/src-tauri/src/lib.rs
git commit -m "feat: add SKILL.md frontmatter parser with tests"
```

---

## Task 6: Deterministic path hashing (TDD)

**Files:** Create `app/src-tauri/src/hash.rs`; Modify `lib.rs` (`mod hash;`).

- [ ] **Step 1: Write the failing test**

Create `app/src-tauri/src/hash.rs`:
```rust
use sha2::{Digest, Sha256};
use std::path::Path;

/// Deterministic SHA-256 over a file's bytes, or a folder's (relative path, bytes)
/// pairs sorted by path. Folder layout changes and content changes both change the hash.
pub fn hash_path(path: &Path) -> std::io::Result<String> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn folder_hash_is_stable_and_order_independent() {
        let a = tempfile::tempdir().unwrap();
        fs::write(a.path().join("SKILL.md"), b"alpha").unwrap();
        fs::create_dir(a.path().join("references")).unwrap();
        fs::write(a.path().join("references/x.md"), b"beta").unwrap();

        let b = tempfile::tempdir().unwrap();
        // write in a different order
        fs::create_dir(b.path().join("references")).unwrap();
        fs::write(b.path().join("references/x.md"), b"beta").unwrap();
        fs::write(b.path().join("SKILL.md"), b"alpha").unwrap();

        assert_eq!(hash_path(a.path()).unwrap(), hash_path(b.path()).unwrap());
    }

    #[test]
    fn content_change_changes_hash() {
        let a = tempfile::tempdir().unwrap();
        fs::write(a.path().join("SKILL.md"), b"one").unwrap();
        let h1 = hash_path(a.path()).unwrap();
        fs::write(a.path().join("SKILL.md"), b"two").unwrap();
        let h2 = hash_path(a.path()).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn hashes_a_single_file() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("agent.md");
        fs::write(&f, b"agent body").unwrap();
        assert_eq!(hash_path(&f).unwrap().len(), 64); // hex sha256
    }
}
```
Add `mod hash;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/src-tauri && cargo test hash::`
Expected: panics with `not implemented`.

- [ ] **Step 3: Implement**

Replace the `hash_path` body and add the walker import at the top (`use walkdir::WalkDir;`):
```rust
use walkdir::WalkDir;

pub fn hash_path(path: &Path) -> std::io::Result<String> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    if path.is_file() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        entries.push((name, std::fs::read(path)?));
    } else {
        for entry in WalkDir::new(path) {
            let entry = entry.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(path)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                entries.push((rel, std::fs::read(entry.path())?));
            }
        }
    }
    entries.sort_by(|x, y| x.0.cmp(&y.0));

    let mut hasher = Sha256::new();
    for (rel, bytes) in entries {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test hash::`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/hash.rs app/src-tauri/src/lib.rs
git commit -m "feat: add deterministic file/folder hashing with tests"
```

---

## Task 7: Database schema & queries (TDD)

**Files:** Create `app/src-tauri/src/db.rs`; Modify `lib.rs` (`mod db;`).

- [ ] **Step 1: Write the failing test**

Create `app/src-tauri/src/db.rs`:
```rust
use crate::model::{Item, ItemType, Location, LocationKind};
use rusqlite::{params, Connection};

pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    unimplemented!()
}

/// Insert a location if its (kind, root_path) is new; return its id either way.
pub fn upsert_location(
    conn: &Connection,
    label: &str,
    root_path: &str,
    kind: LocationKind,
) -> rusqlite::Result<i64> {
    unimplemented!()
}

/// Insert an item if (item_type, slug) is new; return (id, was_new).
pub fn insert_item_if_absent(
    conn: &Connection,
    item_type: ItemType,
    name: &str,
    slug: &str,
    description: &str,
    canonical_hash: &str,
    library_path: &str,
) -> rusqlite::Result<(i64, bool)> {
    unimplemented!()
}

pub fn set_has_variants(conn: &Connection, item_id: i64, value: bool) -> rusqlite::Result<()> {
    unimplemented!()
}

pub fn item_canonical_hash(conn: &Connection, item_id: i64) -> rusqlite::Result<String> {
    unimplemented!()
}

pub fn upsert_placement(
    conn: &Connection,
    item_id: i64,
    location_id: i64,
    rel_path: &str,
    location_hash: &str,
    status: &str,
) -> rusqlite::Result<()> {
    unimplemented!()
}

pub fn list_items(conn: &Connection) -> rusqlite::Result<Vec<Item>> {
    unimplemented!()
}

pub fn list_locations(conn: &Connection) -> rusqlite::Result<Vec<Location>> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_upsert_is_idempotent() {
        let c = open_in_memory().unwrap();
        let id1 = upsert_location(&c, "skills", "/a/b", LocationKind::ClaudeSkills).unwrap();
        let id2 = upsert_location(&c, "skills", "/a/b", LocationKind::ClaudeSkills).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(list_locations(&c).unwrap().len(), 1);
    }

    #[test]
    fn item_insert_then_absent_on_second() {
        let c = open_in_memory().unwrap();
        let (id, new1) =
            insert_item_if_absent(&c, ItemType::Skill, "Babysit", "babysit", "d", "h1", "lib/babysit")
                .unwrap();
        let (id2, new2) =
            insert_item_if_absent(&c, ItemType::Skill, "Babysit", "babysit", "d", "h1", "lib/babysit")
                .unwrap();
        assert!(new1 && !new2);
        assert_eq!(id, id2);
        assert_eq!(item_canonical_hash(&c, id).unwrap(), "h1");
        let items = list_items(&c).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, ItemType::Skill);
    }

    #[test]
    fn placement_upsert_is_idempotent() {
        let c = open_in_memory().unwrap();
        let (item_id, _) =
            insert_item_if_absent(&c, ItemType::Skill, "x", "x", "d", "h", "lib/x").unwrap();
        let loc_id = upsert_location(&c, "skills", "/a", LocationKind::ClaudeSkills).unwrap();
        upsert_placement(&c, item_id, loc_id, "x", "h", "in_sync").unwrap();
        upsert_placement(&c, item_id, loc_id, "x", "h", "in_sync").unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM placements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn has_variants_flag_round_trips() {
        let c = open_in_memory().unwrap();
        let (id, _) = insert_item_if_absent(&c, ItemType::Skill, "x", "x", "d", "h", "lib/x").unwrap();
        set_has_variants(&c, id, true).unwrap();
        assert!(list_items(&c).unwrap()[0].has_variants);
    }
}
```
Add `mod db;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/src-tauri && cargo test db::`
Expected: panics with `not implemented`.

- [ ] **Step 3: Implement the bodies**

Replace each `unimplemented!()` body:
```rust
fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS locations (
            id INTEGER PRIMARY KEY,
            label TEXT NOT NULL,
            root_path TEXT NOT NULL,
            kind TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            last_scanned TEXT,
            UNIQUE(kind, root_path)
        );
        CREATE TABLE IF NOT EXISTS items (
            id INTEGER PRIMARY KEY,
            item_type TEXT NOT NULL,
            name TEXT NOT NULL,
            slug TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            category TEXT,
            subcategory TEXT,
            canonical_hash TEXT NOT NULL,
            library_path TEXT NOT NULL,
            has_variants INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(item_type, slug)
        );
        CREATE TABLE IF NOT EXISTS placements (
            id INTEGER PRIMARY KEY,
            item_id INTEGER NOT NULL REFERENCES items(id),
            location_id INTEGER NOT NULL REFERENCES locations(id),
            rel_path TEXT NOT NULL,
            location_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            last_scanned TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(item_id, location_id)
        );
        ",
    )
}

pub fn upsert_location(
    conn: &Connection,
    label: &str,
    root_path: &str,
    kind: LocationKind,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO locations (label, root_path, kind) VALUES (?1, ?2, ?3)",
        params![label, root_path, kind.as_str()],
    )?;
    conn.query_row(
        "SELECT id FROM locations WHERE kind = ?1 AND root_path = ?2",
        params![kind.as_str(), root_path],
        |r| r.get(0),
    )
}

pub fn insert_item_if_absent(
    conn: &Connection,
    item_type: ItemType,
    name: &str,
    slug: &str,
    description: &str,
    canonical_hash: &str,
    library_path: &str,
) -> rusqlite::Result<(i64, bool)> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO items
            (item_type, name, slug, description, canonical_hash, library_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![item_type.as_str(), name, slug, description, canonical_hash, library_path],
    )?;
    let id = conn.query_row(
        "SELECT id FROM items WHERE item_type = ?1 AND slug = ?2",
        params![item_type.as_str(), slug],
        |r| r.get(0),
    )?;
    Ok((id, changed == 1))
}

pub fn set_has_variants(conn: &Connection, item_id: i64, value: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE items SET has_variants = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![item_id, value as i64],
    )?;
    Ok(())
}

pub fn item_canonical_hash(conn: &Connection, item_id: i64) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT canonical_hash FROM items WHERE id = ?1",
        params![item_id],
        |r| r.get(0),
    )
}

pub fn upsert_placement(
    conn: &Connection,
    item_id: i64,
    location_id: i64,
    rel_path: &str,
    location_hash: &str,
    status: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO placements (item_id, location_id, rel_path, location_hash, status)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(item_id, location_id) DO UPDATE SET
            rel_path = excluded.rel_path,
            location_hash = excluded.location_hash,
            status = excluded.status,
            last_scanned = datetime('now')",
        params![item_id, location_id, rel_path, location_hash, status],
    )?;
    Ok(())
}

pub fn list_items(conn: &Connection) -> rusqlite::Result<Vec<Item>> {
    let mut stmt = conn.prepare(
        "SELECT id, item_type, name, slug, description, category, subcategory,
                canonical_hash, library_path, has_variants
         FROM items ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let type_str: String = r.get(1)?;
        Ok(Item {
            id: r.get(0)?,
            item_type: ItemType::parse(&type_str).unwrap_or(ItemType::Skill),
            name: r.get(2)?,
            slug: r.get(3)?,
            description: r.get(4)?,
            category: r.get(5)?,
            subcategory: r.get(6)?,
            canonical_hash: r.get(7)?,
            library_path: r.get(8)?,
            has_variants: r.get::<_, i64>(9)? != 0,
        })
    })?;
    rows.collect()
}

pub fn list_locations(conn: &Connection) -> rusqlite::Result<Vec<Location>> {
    let mut stmt =
        conn.prepare("SELECT id, label, root_path, kind, enabled FROM locations ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        let kind_str: String = r.get(3)?;
        Ok(Location {
            id: r.get(0)?,
            label: r.get(1)?,
            root_path: r.get(2)?,
            kind: match kind_str.as_str() {
                "marketplace" => LocationKind::Marketplace,
                "agents" => LocationKind::Agents,
                "project" => LocationKind::Project,
                "codex" => LocationKind::Codex,
                "tarball" => LocationKind::Tarball,
                _ => LocationKind::ClaudeSkills,
            },
            enabled: r.get::<_, i64>(4)? != 0,
        })
    })?;
    rows.collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test db::`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/db.rs app/src-tauri/src/lib.rs
git commit -m "feat: add SQLite schema and item/location/placement queries with tests"
```

---

## Task 8: Scanner (TDD)

**Files:** Create `app/src-tauri/src/scanner.rs`; Modify `lib.rs` (`mod scanner;`).

- [ ] **Step 1: Write the failing test**

Create `app/src-tauri/src/scanner.rs`:
```rust
use crate::hash::hash_path;
use crate::meta::parse_meta;
use crate::model::{ItemType, LocationKind, ScannedItem};
use std::path::Path;
use walkdir::WalkDir;

/// Find skills (folders with SKILL.md) or agents (top-level *.md) under `root`.
pub fn scan_location(root: &Path, kind: LocationKind) -> std::io::Result<Vec<ScannedItem>> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        let found = scan_location(Path::new("/no/such/dir/xyz"), LocationKind::ClaudeSkills).unwrap();
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
```
Add `mod scanner;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/src-tauri && cargo test scanner::`
Expected: panics with `not implemented`.

- [ ] **Step 3: Implement**

Replace the `scan_location` body:
```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test scanner::`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/scanner.rs app/src-tauri/src/lib.rs
git commit -m "feat: add location scanner for skills and agents with tests"
```

---

## Task 9: Importer — copy into library, record placements (TDD)

**Files:** Create `app/src-tauri/src/importer.rs`; Modify `lib.rs` (`mod importer;`).

This task wires scan → library copy → DB. Tarball extraction is Step 6 (added after the core import loop is proven).

- [ ] **Step 1: Write the failing test (core import loop)**

Create `app/src-tauri/src/importer.rs`:
```rust
use crate::db;
use crate::hash::hash_path;
use crate::model::{ImportSummary, ItemType, LocationKind, ScannedItem};
use crate::scanner::scan_location;
use crate::slug::slugify;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Copy a scanned item into the library and upsert item + placement.
/// Returns whether the item row was newly created.
pub fn import_scanned(
    conn: &Connection,
    library_root: &Path,
    location_id: i64,
    location_root: &Path,
    scanned: &ScannedItem,
    summary: &mut ImportSummary,
) -> std::io::Result<()> {
    unimplemented!()
}

/// Recursively copy a file or directory tree to `dst`.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    unimplemented!()
}

fn library_dest(library_root: &Path, item_type: ItemType, slug: &str) -> PathBuf {
    library_root
        .join("_uncategorized")
        .join(item_type.as_str())
        .join(slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scanned_skill(dir: &Path, name: &str, body: &str) -> ScannedItem {
        let folder = dir.join(name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), body).unwrap();
        ScannedItem {
            item_type: ItemType::Skill,
            name: name.to_string(),
            description: String::new(),
            hash: hash_path(&folder).unwrap(),
            source_path: folder,
        }
    }

    #[test]
    fn imports_new_item_and_copies_folder() {
        let conn = db::open_in_memory().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let loc_id = db::upsert_location(&conn, "skills", src.path().to_str().unwrap(), LocationKind::ClaudeSkills).unwrap();
        let item = scanned_skill(src.path(), "babysit", "---\nname: babysit\n---\n");
        let mut s = ImportSummary::default();

        import_scanned(&conn, lib.path(), loc_id, src.path(), &item, &mut s).unwrap();

        assert_eq!(s.items_new, 1);
        assert_eq!(s.placements_recorded, 1);
        assert!(lib.path().join("_uncategorized/skill/babysit/SKILL.md").exists());
        assert_eq!(db::list_items(&conn).unwrap().len(), 1);
    }

    #[test]
    fn second_identical_source_is_in_sync_not_variant() {
        let conn = db::open_in_memory().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let loc1 = db::upsert_location(&conn, "a", "/a", LocationKind::ClaudeSkills).unwrap();
        let loc2 = db::upsert_location(&conn, "b", "/b", LocationKind::Codex).unwrap();
        let item = scanned_skill(src.path(), "x", "---\nname: x\n---\nsame\n");
        let mut s = ImportSummary::default();
        import_scanned(&conn, lib.path(), loc1, src.path(), &item, &mut s).unwrap();
        import_scanned(&conn, lib.path(), loc2, src.path(), &item, &mut s).unwrap();
        assert_eq!(s.items_new, 1);
        assert_eq!(s.variants_flagged, 0);
        assert!(!db::list_items(&conn).unwrap()[0].has_variants);
    }

    #[test]
    fn second_differing_source_flags_variant() {
        let conn = db::open_in_memory().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let src1 = tempfile::tempdir().unwrap();
        let src2 = tempfile::tempdir().unwrap();
        let loc1 = db::upsert_location(&conn, "a", "/a", LocationKind::ClaudeSkills).unwrap();
        let loc2 = db::upsert_location(&conn, "b", "/b", LocationKind::Codex).unwrap();
        let a = scanned_skill(src1.path(), "x", "---\nname: x\n---\nversion A\n");
        let b = scanned_skill(src2.path(), "x", "---\nname: x\n---\nversion B DIFFERENT\n");
        let mut s = ImportSummary::default();
        import_scanned(&conn, lib.path(), loc1, src1.path(), &a, &mut s).unwrap();
        import_scanned(&conn, lib.path(), loc2, src2.path(), &b, &mut s).unwrap();
        assert_eq!(s.variants_flagged, 1);
        assert!(db::list_items(&conn).unwrap()[0].has_variants);
    }
}
```
Add `mod importer;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/src-tauri && cargo test importer::`
Expected: panics with `not implemented`.

- [ ] **Step 3: Implement the core loop + copy**

Replace the two `unimplemented!()` bodies:
```rust
pub fn import_scanned(
    conn: &Connection,
    library_root: &Path,
    location_id: i64,
    location_root: &Path,
    scanned: &ScannedItem,
    summary: &mut ImportSummary,
) -> std::io::Result<()> {
    summary.items_found += 1;
    let slug = {
        let s = slugify(&scanned.name);
        if s.is_empty() { "unnamed".to_string() } else { s }
    };
    let dest = library_dest(library_root, scanned.item_type, &slug);
    let lib_path_str = dest.to_string_lossy().to_string();

    let to_db = |e: rusqlite::Error| std::io::Error::new(std::io::ErrorKind::Other, e);

    let (item_id, was_new) = db::insert_item_if_absent(
        conn,
        scanned.item_type,
        &scanned.name,
        &slug,
        &scanned.description,
        &scanned.hash,
        &lib_path_str,
    )
    .map_err(to_db)?;

    if was_new {
        summary.items_new += 1;
        if dest.exists() {
            std::fs::remove_dir_all(&dest).ok();
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        copy_tree(&scanned.source_path, &dest)?;
    }

    let canonical = db::item_canonical_hash(conn, item_id).map_err(to_db)?;
    let status = if scanned.hash == canonical { "in_sync" } else { "conflict" };
    if status == "conflict" {
        db::set_has_variants(conn, item_id, true).map_err(to_db)?;
        summary.variants_flagged += 1;
    }

    let rel_path = scanned
        .source_path
        .strip_prefix(location_root)
        .unwrap_or(&scanned.source_path)
        .to_string_lossy()
        .replace('\\', "/");

    db::upsert_placement(conn, item_id, location_id, &rel_path, &scanned.hash, status)
        .map_err(to_db)?;
    summary.placements_recorded += 1;
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let child = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &child)?;
        } else {
            std::fs::copy(entry.path(), &child)?;
        }
    }
    Ok(())
}
```
Note: agents are single files; `library_dest` is a directory, so for agents `copy_tree` copies the `.md` into the slug folder. Adjust the agent dest to a file when `item_type == Agent`: change `import_scanned` to compute `dest` as `library_dest(...).join("agent.md")` for agents. Apply this now:
```rust
    let dest = if scanned.item_type == ItemType::Agent {
        library_dest(library_root, scanned.item_type, &slug).join(format!("{slug}.md"))
    } else {
        library_dest(library_root, scanned.item_type, &slug)
    };
```
(Replace the earlier single-line `let dest = library_dest(...)`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test importer::`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/importer.rs app/src-tauri/src/lib.rs
git commit -m "feat: import scanned items into library with variant flagging (tests)"
```

- [ ] **Step 6: Add tarball import (TDD)**

Append to `importer.rs` above the `#[cfg(test)]` module:
```rust
use flate2::read::GzDecoder;
use tar::Archive;

/// Extract a `skills-deduped.tar.gz` of `**/SKILL.md` trees into a staging dir,
/// then import each discovered skill against the given tarball location id.
pub fn import_tarball(
    conn: &Connection,
    library_root: &Path,
    tarball_location_id: i64,
    tarball_path: &Path,
    staging_dir: &Path,
    summary: &mut ImportSummary,
) -> std::io::Result<()> {
    std::fs::create_dir_all(staging_dir)?;
    let file = std::fs::File::open(tarball_path)?;
    Archive::new(GzDecoder::new(file)).unpack(staging_dir)?;
    for scanned in scan_location(staging_dir, LocationKind::ClaudeSkills)? {
        import_scanned(conn, library_root, tarball_location_id, staging_dir, &scanned, summary)?;
    }
    Ok(())
}
```
Add this test inside the `tests` module:
```rust
    #[test]
    fn imports_skills_from_a_tarball() {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;

        // build a tiny tar.gz containing one skill folder
        let tmp = tempfile::tempdir().unwrap();
        let tgz = tmp.path().join("skills.tar.gz");
        {
            let f = std::fs::File::create(&tgz).unwrap();
            let enc = GzEncoder::new(f, Compression::default());
            let mut tar = tar::Builder::new(enc);
            let mut header = tar::Header::new_gnu();
            let body = b"---\nname: packed\ndescription: from tar\n---\n";
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "packed/SKILL.md", &body[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }

        let conn = db::open_in_memory().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("staging");
        let loc = db::upsert_location(&conn, "tarball", tgz.to_str().unwrap(), LocationKind::Tarball).unwrap();
        let mut s = ImportSummary::default();

        import_tarball(&conn, lib.path(), loc, &tgz, &staging, &mut s).unwrap();

        assert_eq!(s.items_new, 1);
        assert!(lib.path().join("_uncategorized/skill/packed/SKILL.md").exists());
    }
```

- [ ] **Step 7: Run, verify, commit**

Run: `cd app/src-tauri && cargo test importer::`
Expected: 4 passed.
```bash
git add app/src-tauri/src/importer.rs
git commit -m "feat: import skills from skills-deduped.tar.gz with test"
```

---

## Task 10: App state, default locations, and Tauri commands

**Files:** Create `app/src-tauri/src/commands.rs`; Modify `app/src-tauri/src/lib.rs`.

- [ ] **Step 1: Write default-location discovery (TDD)**

Create `app/src-tauri/src/commands.rs`:
```rust
use crate::model::{Item, Location, LocationKind};
use std::path::{Path, PathBuf};

/// Build the default set of (label, path, kind) candidates relative to a home dir.
/// Only paths that exist are returned (except the tarball, handled separately).
pub fn default_location_candidates(home: &Path) -> Vec<(String, PathBuf, LocationKind)> {
    let mut out = vec![
        ("Claude skills".into(), home.join(".claude/skills"), LocationKind::ClaudeSkills),
        ("Claude agents".into(), home.join(".claude/agents"), LocationKind::Agents),
        ("Marketplaces".into(), home.join(".claude/plugins/marketplaces"), LocationKind::Marketplace),
        ("Codex skills".into(), home.join(".codex/skills"), LocationKind::Codex),
    ];
    out.retain(|(_, p, _)| p.exists());
    out
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
}
```
Add `mod commands;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it passes**

Run: `cd app/src-tauri && cargo test commands::`
Expected: 1 passed.

- [ ] **Step 3: Add the AppState struct and Tauri commands**

Append to `commands.rs`:
```rust
use crate::{db, importer};
use std::sync::Mutex;
use tauri::{Manager, State};

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

#[tauri::command]
pub fn run_import(state: State<AppState>) -> Result<crate::model::ImportSummary, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut summary = crate::model::ImportSummary::default();

    for (label, path, kind) in default_location_candidates(&state.home) {
        let path_str = path.to_string_lossy().to_string();
        let loc_id = db::upsert_location(&conn, &label, &path_str, kind).map_err(|e| e.to_string())?;
        let scanned = crate::scanner::scan_location(&path, kind).map_err(|e| e.to_string())?;
        summary.locations_scanned += 1;
        for item in &scanned {
            importer::import_scanned(&conn, &state.library_root, loc_id, &path, item, &mut summary)
                .map_err(|e| e.to_string())?;
        }
    }

    if let Some(tarball) = &state.tarball_path {
        if tarball.exists() {
            let loc_id = db::upsert_location(
                &conn, "Inventory tarball", &tarball.to_string_lossy(), LocationKind::Tarball,
            ).map_err(|e| e.to_string())?;
            let staging = state.library_root.join("_staging");
            importer::import_tarball(&conn, &state.library_root, loc_id, tarball, &staging, &mut summary)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(summary)
}
```

- [ ] **Step 4: Wire state + handlers in `lib.rs`**

Replace the body of the generated `run()` in `app/src-tauri/src/lib.rs` so it builds `AppState` in `.setup()` and registers the commands. The full `run()` should read:
```rust
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let home = dirs::home_dir().expect("home dir");
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("app data dir")
                .join("skill-library");
            let library_root = data_dir.join("library");
            std::fs::create_dir_all(&library_root).expect("create library dir");
            let conn = db::open(&data_dir.join("catalog.db")).expect("open db");

            // The bundled inventory tarball, if present in the repo.
            let tarball_path = home
                .join("Repo/skills/skills-inventory/skills-deduped.tar.gz");
            let tarball_path = if tarball_path.exists() { Some(tarball_path) } else { None };

            app.manage(commands::AppState {
                db: std::sync::Mutex::new(conn),
                library_root,
                home,
                tarball_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::list_locations,
            commands::run_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```
Ensure the top of `lib.rs` has all module declarations:
```rust
mod commands;
mod db;
mod hash;
mod importer;
mod meta;
mod model;
mod scanner;
mod slug;
```

- [ ] **Step 5: Verify it compiles and all tests pass**

Run: `cd app/src-tauri && cargo test`
Expected: every test from Tasks 4–10 passes; `cargo build` clean.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/commands.rs app/src-tauri/src/lib.rs
git commit -m "feat: add AppState, default-location discovery, and import/list commands"
```

---

## Task 11: Minimal frontend (list + import button)

**Files:** Create `app/src/api.ts`; Modify `app/src/main.ts`, `app/index.html`. Remove scaffold demo bits as needed.

- [ ] **Step 1: Typed API wrappers**

Create `app/src/api.ts`:
```ts
import { invoke } from "@tauri-apps/api/core";

export type ItemType = "skill" | "agent";

export interface Item {
  id: number;
  item_type: ItemType;
  name: string;
  slug: string;
  description: string;
  category: string | null;
  subcategory: string | null;
  canonical_hash: string;
  library_path: string;
  has_variants: boolean;
}

export interface ImportSummary {
  locations_scanned: number;
  items_found: number;
  items_new: number;
  placements_recorded: number;
  variants_flagged: number;
}

export const listItems = () => invoke<Item[]>("list_items");
export const runImport = () => invoke<ImportSummary>("run_import");
```

- [ ] **Step 2: Markup**

Replace the `<body>` contents of `app/index.html` with:
```html
<body>
  <main class="container">
    <h1>Skill &amp; Agent Library</h1>
    <button id="import">Scan &amp; import</button>
    <p id="status"></p>
    <ul id="items"></ul>
  </main>
  <script type="module" src="/src/main.ts"></script>
</body>
```

- [ ] **Step 3: Wire the UI**

Replace `app/src/main.ts` with:
```ts
import { listItems, runImport, type Item } from "./api";

const statusEl = document.getElementById("status")!;
const listEl = document.getElementById("items")!;
const importBtn = document.getElementById("import") as HTMLButtonElement;

function render(items: Item[]) {
  listEl.innerHTML = "";
  for (const it of items) {
    const li = document.createElement("li");
    const variant = it.has_variants ? " ⚠ variants" : "";
    li.textContent = `[${it.item_type}] ${it.name}${variant} — ${it.description}`;
    listEl.appendChild(li);
  }
  statusEl.textContent = `${items.length} items in library`;
}

async function refresh() {
  render(await listItems());
}

importBtn.addEventListener("click", async () => {
  importBtn.disabled = true;
  statusEl.textContent = "Importing…";
  try {
    const s = await runImport();
    statusEl.textContent =
      `Scanned ${s.locations_scanned} locations · ${s.items_new} new · ${s.variants_flagged} variant-flagged`;
    await refresh();
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    importBtn.disabled = false;
  }
});

refresh().catch((e) => (statusEl.textContent = `Error: ${e}`));
```
(If the scaffold's `main.ts` imported a `styles.css`, keep that import line at the top.)

- [ ] **Step 4: Commit**

```bash
git add app/src/api.ts app/src/main.ts app/index.html
git commit -m "feat: minimal frontend — import button and item list"
```

---

## Task 12: End-to-end verification

**Files:** none (manual run).

- [ ] **Step 1: Launch the app**

Run: `cd app && npm run tauri dev`
Expected: window opens, empty list, status blank.

- [ ] **Step 2: Import**

Click **Scan & import**. Expected: status shows locations scanned and a non-zero item count; the list fills with `[skill] …` / `[agent] …` rows; items that exist in multiple differing copies show `⚠ variants`.

- [ ] **Step 3: Confirm persistence**

Close and relaunch (`npm run tauri dev`). Expected: the list is populated immediately from the DB without re-importing (the catalog persisted under the OS app-data dir).

- [ ] **Step 4: Confirm library copies on disk**

Verify the library folder exists and contains copied skill folders:
```bash
ls "$APPDATA/com.skill-library.app/skill-library/library/_uncategorized/skill" | head
```
(The exact app-data path depends on the bundle identifier in `tauri.conf.json`; adjust if different.)
Expected: a list of slug-named folders, each containing `SKILL.md`.

- [ ] **Step 5: Final commit / branch state**

```bash
git status   # working tree clean
git log --oneline -12
```
Expected: a clean history of the milestone's commits on `skill-library-app`.

---

## Self-review notes (author)

- **Spec coverage:** scanner (locations), library store (SQLite + copied folders), seed (tarball + live scan), agents (type field + agents scan mode), whole-folder hashing, variant flagging — all have tasks. Classification, sync, and merge are explicitly deferred to later milestone plans per the spec's build order.
- **Type consistency:** `ItemType`/`LocationKind` (Rust) mirror `ItemType` and the kind strings used in `db.rs`; `Item`/`ImportSummary` field names match the TS interfaces in `api.ts`.
- **Deferred/known limits (carried to later plans):** `conflict` status is a coarse "hashes differ" flag in M1 (true newer/older direction + variant capture rows arrive with the sync/merge milestones); `parse_meta` handles single-line scalars only.
