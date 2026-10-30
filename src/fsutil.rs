//! Path and file helpers.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Expand a leading `~` (or `~/…`) to *home*. Other paths are returned as-is.
pub fn expand_tilde(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        home.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    }
}

/// Make *p* absolute and normalise it like Python's `Path.resolve()`:
/// symlinks are resolved for the part of the path that exists, and the path
/// does not need to exist (unlike `fs::canonicalize`).
pub fn resolve_lenient(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    };

    // Lexically drop `.` and `..` first.
    let mut norm = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                norm.pop();
            }
            other => norm.push(other.as_os_str()),
        }
    }

    // Canonicalize the deepest existing ancestor and re-append the rest.
    let mut existing = norm.as_path();
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    loop {
        if let Ok(canon) = fs::canonicalize(existing) {
            let mut out = canon;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name);
                existing = parent;
            }
            _ => return norm,
        }
    }
}

/// `path` relative to `base`, if `base` is a prefix of it (component-wise).
pub fn relative_to(path: &Path, base: &Path) -> Option<PathBuf> {
    path.strip_prefix(base).ok().map(Path::to_path_buf)
}

/// True if *p* is an existing regular file (following symlinks).
pub fn is_file(p: &Path) -> bool {
    p.is_file()
}

/// True if something (including a dangling symlink) exists at *p*.
pub fn lexists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

/// Read *p* if it is a regular file.
pub fn read_file(p: &Path) -> Option<Vec<u8>> {
    if is_file(p) {
        fs::read(p).ok()
    } else {
        None
    }
}

/// Outcome of [`apply_copy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyStatus {
    /// Target already identical; nothing written.
    Ok,
    /// Target written (or would be, in a dry run).
    Copied,
}

/// Write *content* to *target* as a plain copy. Replaces existing files and
/// dangling symlinks rather than writing through them.
pub fn apply_copy(content: &[u8], target: &Path, dry: bool) -> io::Result<CopyStatus> {
    if read_file(target).as_deref() == Some(content) {
        return Ok(CopyStatus::Ok);
    }
    if !dry {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if lexists(target) {
            fs::remove_file(target)?;
        }
        fs::write(target, content)?;
    }
    Ok(CopyStatus::Copied)
}

/// Copy *src* to *dest*, creating parent dirs and keeping permissions.
pub fn copy_file(src: &Path, dest: &Path) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest).map(|_| ())
}

/// All regular files under *dir*, sorted, skipping anything inside a `.git` dir.
pub fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde() {
        let home = Path::new("/h");
        assert_eq!(expand_tilde("~", home), PathBuf::from("/h"));
        assert_eq!(expand_tilde("~/.bashrc", home), PathBuf::from("/h/.bashrc"));
        assert_eq!(expand_tilde("/etc/x", home), PathBuf::from("/etc/x"));
        assert_eq!(expand_tilde("~user/x", home), PathBuf::from("~user/x"));
    }

    #[test]
    fn resolves_paths_that_do_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let missing = tmp.path().join("a/./b/../c.txt");
        assert_eq!(resolve_lenient(&missing), base.join("a/c.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn resolves_symlinked_ancestors() {
        let tmp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        fs::create_dir(base.join("real")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
        assert_eq!(
            resolve_lenient(&base.join("link/new.txt")),
            base.join("real/new.txt")
        );
    }

    #[test]
    fn apply_copy_reports_ok_copied_and_respects_dry_run() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("sub/file");
        assert_eq!(apply_copy(b"x", &target, true).unwrap(), CopyStatus::Copied);
        assert!(!target.exists());
        assert_eq!(
            apply_copy(b"x", &target, false).unwrap(),
            CopyStatus::Copied
        );
        assert_eq!(fs::read(&target).unwrap(), b"x");
        assert_eq!(apply_copy(b"x", &target, false).unwrap(), CopyStatus::Ok);
    }

    #[cfg(unix)]
    #[test]
    fn apply_copy_replaces_dangling_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("link");
        std::os::unix::fs::symlink(tmp.path().join("nowhere"), &target).unwrap();
        apply_copy(b"x", &target, false).unwrap();
        assert!(!fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"x");
    }

    #[test]
    fn walk_files_sorts_and_skips_git() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::create_dir_all(d.join("a")).unwrap();
        fs::write(d.join(".git/HEAD"), "x").unwrap();
        fs::write(d.join("a/b"), "x").unwrap();
        fs::write(d.join("a.txt"), "x").unwrap();
        let rel: Vec<_> = walk_files(d)
            .into_iter()
            .map(|p| p.strip_prefix(d).unwrap().to_path_buf())
            .collect();
        assert_eq!(rel, vec![PathBuf::from("a/b"), PathBuf::from("a.txt")]);
    }
}
