use crate::model::{Item, ItemType, Location, LocationKind, ScanDir};
use rusqlite::{params, Connection};

pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn)?;
    Ok(conn)
}

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
        CREATE TABLE IF NOT EXISTS scan_dirs (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL,
            item_type TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            UNIQUE(path, item_type)
        );
        ",
    )
}

/// Insert a location if its (kind, root_path) is new; return its id either way.
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
    let changed = conn.execute(
        "INSERT OR IGNORE INTO items
            (item_type, name, slug, description, canonical_hash, library_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            item_type.as_str(),
            name,
            slug,
            description,
            canonical_hash,
            library_path
        ],
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

pub fn item_library_path(conn: &Connection, item_id: i64) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT library_path FROM items WHERE id = ?1",
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

/// Insert a user scan dir if (path, item_type) is new; return its id.
pub fn add_scan_dir(conn: &Connection, path: &str, item_type: ItemType) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO scan_dirs (path, item_type) VALUES (?1, ?2)",
        params![path, item_type.as_str()],
    )?;
    conn.query_row(
        "SELECT id FROM scan_dirs WHERE path = ?1 AND item_type = ?2",
        params![path, item_type.as_str()],
        |r| r.get(0),
    )
}

pub fn list_scan_dirs(conn: &Connection) -> rusqlite::Result<Vec<ScanDir>> {
    let mut stmt = conn.prepare("SELECT id, path, item_type, enabled FROM scan_dirs ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        let t: String = r.get(2)?;
        Ok(ScanDir {
            id: r.get(0)?,
            path: r.get(1)?,
            item_type: ItemType::parse(&t).unwrap_or(ItemType::Skill),
            enabled: r.get::<_, i64>(3)? != 0,
        })
    })?;
    rows.collect()
}

pub fn remove_scan_dir(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM scan_dirs WHERE id = ?1", params![id])?;
    Ok(())
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
        let (id, new1) = insert_item_if_absent(
            &c,
            ItemType::Skill,
            "Babysit",
            "babysit",
            "d",
            "h1",
            "lib/babysit",
        )
        .unwrap();
        let (id2, new2) = insert_item_if_absent(
            &c,
            ItemType::Skill,
            "Babysit",
            "babysit",
            "d",
            "h1",
            "lib/babysit",
        )
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
        let (id, _) =
            insert_item_if_absent(&c, ItemType::Skill, "x", "x", "d", "h", "lib/x").unwrap();
        set_has_variants(&c, id, true).unwrap();
        assert!(list_items(&c).unwrap()[0].has_variants);
    }

    #[test]
    fn scan_dirs_crud() {
        let c = open_in_memory().unwrap();
        let id = add_scan_dir(&c, "/my/agents", ItemType::Agent).unwrap();
        add_scan_dir(&c, "/my/agents", ItemType::Agent).unwrap(); // idempotent
        let dirs = list_scan_dirs(&c).unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].item_type, ItemType::Agent);
        assert_eq!(dirs[0].path, "/my/agents");
        remove_scan_dir(&c, id).unwrap();
        assert!(list_scan_dirs(&c).unwrap().is_empty());
    }
}
