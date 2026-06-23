use sha2::{Digest, Sha256};
use std::path::Path;
use walkdir::WalkDir;

/// Deterministic SHA-256 over a file's bytes, or a folder's (relative path, bytes)
/// pairs sorted by path. Folder layout changes and content changes both change the hash.
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
