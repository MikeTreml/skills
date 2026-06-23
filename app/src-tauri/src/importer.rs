use crate::db;
use crate::model::{ImportSummary, ItemType, LocationKind, ScannedItem};
use crate::scanner::scan_location;
use crate::slug::slugify;
use flate2::read::GzDecoder;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use tar::Archive;

/// Copy a scanned item into the library and upsert item + placement.
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
        if s.is_empty() {
            "unnamed".to_string()
        } else {
            s
        }
    };
    // A file source (agent .md, or a single-file skill) lands as <slug>.md inside
    // its library folder; a directory source (folder skill) copies the whole folder.
    let dest = if scanned.source_path.is_file() {
        library_dest(library_root, scanned.item_type, &slug).join(format!("{slug}.md"))
    } else {
        library_dest(library_root, scanned.item_type, &slug)
    };
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
    let status = if scanned.hash == canonical {
        "in_sync"
    } else {
        "conflict"
    };
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

/// Recursively copy a file or directory tree to `dst`.
pub fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
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

fn library_dest(library_root: &Path, item_type: ItemType, slug: &str) -> PathBuf {
    library_root
        .join("_uncategorized")
        .join(item_type.as_str())
        .join(slug)
}

/// Extract a `skills-deduped.tar.gz` of `**/SKILL.md` trees into a staging dir,
/// then import each discovered skill against the given tarball location id.
pub fn import_tarball(
    conn: &Connection,
    library_root: &Path,
    tarball_location_id: i64,
    tarball_path: &Path,
    staging_dir: &Path,
    summary: &mut ImportSummary,
    report: &dyn Fn(String),
    is_cancelled: &dyn Fn() -> bool,
) -> std::io::Result<()> {
    report("Extracting tarball…".to_string());
    std::fs::create_dir_all(staging_dir)?;
    // Don't even start the multi-second unpack if cancellation is already pending.
    if is_cancelled() {
        std::fs::remove_dir_all(staging_dir).ok();
        summary.cancelled = true;
        return Ok(());
    }
    let file = std::fs::File::open(tarball_path)?;
    Archive::new(GzDecoder::new(file)).unpack(staging_dir)?;
    let scanned = scan_location(staging_dir, LocationKind::ClaudeSkills)?;
    let total = scanned.len();
    for (i, item) in scanned.iter().enumerate() {
        // Checked every iteration (an atomic load is free) so Cancel feels instant.
        // The i==0 check also covers cancellation during unpack/scan above.
        // Always lands BETWEEN whole-item writes — each import_scanned autocommits.
        if is_cancelled() {
            std::fs::remove_dir_all(staging_dir).ok();
            summary.cancelled = true;
            return Ok(());
        }
        import_scanned(conn, library_root, tarball_location_id, staging_dir, item, summary)?;
        if i % 200 == 0 {
            report(format!("Importing tarball… {i}/{total}"));
        }
    }
    // The extracted tarball can be several MB; remove it now that everything
    // has been copied into the library so it doesn't accumulate on disk.
    std::fs::remove_dir_all(staging_dir).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::hash_path;
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
        let loc_id = db::upsert_location(
            &conn,
            "skills",
            src.path().to_str().unwrap(),
            LocationKind::ClaudeSkills,
        )
        .unwrap();
        let item = scanned_skill(src.path(), "babysit", "---\nname: babysit\n---\n");
        let mut s = ImportSummary::default();

        import_scanned(&conn, lib.path(), loc_id, src.path(), &item, &mut s).unwrap();

        assert_eq!(s.items_new, 1);
        assert_eq!(s.placements_recorded, 1);
        assert!(lib
            .path()
            .join("_uncategorized/skill/babysit/SKILL.md")
            .exists());
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

    #[test]
    fn imports_skills_from_a_tarball() {
        use flate2::{write::GzEncoder, Compression};

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
            tar.append_data(&mut header, "packed/SKILL.md", &body[..])
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }

        let conn = db::open_in_memory().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("staging");
        let loc = db::upsert_location(
            &conn,
            "tarball",
            tgz.to_str().unwrap(),
            LocationKind::Tarball,
        )
        .unwrap();
        let mut s = ImportSummary::default();

        import_tarball(&conn, lib.path(), loc, &tgz, &staging, &mut s, &|_| {}, &|| false).unwrap();

        assert_eq!(s.items_new, 1);
        assert!(lib
            .path()
            .join("_uncategorized/skill/packed/SKILL.md")
            .exists());
        assert!(
            !staging.exists(),
            "staging dir should be cleaned up after import_tarball returns"
        );
    }

    #[test]
    fn tarball_import_honors_cancel_midway() {
        use flate2::{write::GzEncoder, Compression};
        use std::sync::atomic::{AtomicUsize, Ordering};

        // A tar.gz with three skill folders (scanned in sorted order a, b, c).
        let tmp = tempfile::tempdir().unwrap();
        let tgz = tmp.path().join("skills.tar.gz");
        {
            let f = std::fs::File::create(&tgz).unwrap();
            let enc = GzEncoder::new(f, Compression::default());
            let mut tar = tar::Builder::new(enc);
            for name in ["a", "b", "c"] {
                let mut header = tar::Header::new_gnu();
                let body = format!("---\nname: {name}\n---\n");
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar.append_data(&mut header, format!("{name}/SKILL.md"), body.as_bytes())
                    .unwrap();
            }
            tar.into_inner().unwrap().finish().unwrap();
        }

        let conn = db::open_in_memory().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("staging");
        let loc =
            db::upsert_location(&conn, "tarball", tgz.to_str().unwrap(), LocationKind::Tarball)
                .unwrap();
        let mut s = ImportSummary::default();

        // Checks: [0] pre-unpack (false), [1] loop i=0 (false → import item a),
        // [2] loop i=1 (true → stop before item b). So exactly one item imports.
        let calls = AtomicUsize::new(0);
        import_tarball(
            &conn,
            lib.path(),
            loc,
            &tgz,
            &staging,
            &mut s,
            &|_| {},
            &|| calls.fetch_add(1, Ordering::SeqCst) >= 2,
        )
        .unwrap();

        assert_eq!(s.items_new, 1, "should stop after the first item");
        assert!(s.cancelled, "summary flags the early stop");
        assert_eq!(db::list_items(&conn).unwrap().len(), 1, "partial catalog is valid");
        assert!(!staging.exists(), "staging dir cleaned up on cancel");
    }
}
