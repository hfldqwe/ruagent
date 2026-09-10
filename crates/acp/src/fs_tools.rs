//! Client-side filesystem tools (`fs/read_text_file`, `fs/write_text_file`).
//!
//! ACP agents may ask the client to read/write files. We serve those from
//! the run's working directory ONLY — path escapes are rejected. This is a
//! client-role convenience boundary, not a sandbox (design §8.2: the honest
//! boundary is the daemon's OS user).

use std::path::{Path, PathBuf};

/// Canonicalize `root`.
fn canon_root(root: &Path) -> std::io::Result<PathBuf> {
    root.canonicalize()
}

/// Resolve an existing file path and enforce it lives inside `root`.
/// Symlinks are resolved by canonicalization, so a link pointing outside
/// is rejected.
pub fn resolve_existing_under(root: &Path, path: &Path) -> std::io::Result<PathBuf> {
    let root = canon_root(root)?;
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical = target.canonicalize().map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("cannot resolve {}: {e}", target.display()),
        )
    })?;
    if !canonical.starts_with(&root) {
        return Err(std::io::Error::other(format!(
            "path {} escapes the run workspace",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Resolve a not-yet-existing file path inside `root` (for writes):
/// canonicalize the parent, then re-attach the file name.
pub fn resolve_new_under(root: &Path, path: &Path) -> std::io::Result<PathBuf> {
    let root = canon_root(root)?;
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent"))?;
    let canonical_parent = parent.canonicalize().map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("cannot resolve {}: {e}", parent.display()),
        )
    })?;
    if !canonical_parent.starts_with(&root) {
        return Err(std::io::Error::other(format!(
            "path {} escapes the run workspace",
            canonical_parent.display()
        )));
    }
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("path ends in .."))?;
    Ok(canonical_parent.join(name))
}

/// Apply the optional 1-based `line`/`limit` window of `fs/read_text_file`.
pub fn apply_line_window(content: &str, line: Option<u32>, limit: Option<u32>) -> String {
    match (line, limit) {
        (None, None) => content.to_string(),
        _ => {
            let start = line.unwrap_or(1).saturating_sub(1) as usize;
            let take = limit.map(|l| l as usize);
            content
                .lines()
                .skip(start)
                .take(take.unwrap_or(usize::MAX))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ruagent-fs-test-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn existing_file_inside_root_resolves() {
        let root = temp_root("in");
        let file = root.join("a.txt");
        std::fs::write(&file, "hello").unwrap();
        let resolved = resolve_existing_under(&root, &file).unwrap();
        assert!(resolved.starts_with(root.canonicalize().unwrap()));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn escape_is_rejected() {
        let root = temp_root("esc");
        let outside = std::env::temp_dir().join("ruagent-fs-test-outside.txt");
        let err = resolve_existing_under(&root, &outside);
        assert!(err.is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn new_file_resolves_under_root() {
        let root = temp_root("new");
        let target = root.join("sub").join("b.txt");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let resolved = resolve_new_under(&root, &target).unwrap();
        assert!(resolved.ends_with("b.txt"));
        // Traversal above root is rejected.
        let evil = root.join("..").join("evil.txt");
        assert!(resolve_new_under(&root, &evil).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn line_window_slices() {
        let content = "l1\nl2\nl3\nl4";
        assert_eq!(apply_line_window(content, None, None), content);
        assert_eq!(apply_line_window(content, Some(2), Some(2)), "l2\nl3");
        assert_eq!(apply_line_window(content, Some(3), None), "l3\nl4");
    }
}
