//! Read and write helpers for the `.secrets/` tree.
//!
//! Two invariants, enforced here rather than remembered at each call site:
//!
//! 1. Everything created is mode 0600.
//! 2. Nothing is written outside `.secrets/`.
//!
//! T-01-03: the organization root private key lives under this tree. It is
//! gitignored and dockerignored, but a stray write to a path outside the tree
//! would escape both, so the boundary is checked in code too.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Root of the secrets tree, overridable for the containerised tools.
#[must_use]
pub fn secrets_dir() -> PathBuf {
    std::env::var("DOMO_SECRETS_DIR")
        .map_or_else(|_| PathBuf::from(".secrets"), PathBuf::from)
}

/// Resolve `relative` inside the secrets tree, refusing to escape it.
pub fn path(relative: impl AsRef<Path>) -> Result<PathBuf> {
    let relative = relative.as_ref();
    anyhow::ensure!(
        relative.is_relative(),
        "secrets path must be relative, got {}",
        relative.display()
    );
    anyhow::ensure!(
        !relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        "secrets path must not climb out of the tree: {}",
        relative.display()
    );
    Ok(secrets_dir().join(relative))
}

/// Read a file from the secrets tree.
pub fn read(relative: impl AsRef<Path>) -> Result<Vec<u8>> {
    let p = path(relative)?;
    fs::read(&p).with_context(|| format!("reading {}", p.display()))
}

/// Read a file from the secrets tree as UTF-8, trimming trailing newlines.
pub fn read_string(relative: impl AsRef<Path>) -> Result<String> {
    let p = path(relative)?;
    let s = fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    Ok(s.trim_end().to_owned())
}

/// True when the named secret already exists.
#[must_use]
pub fn exists(relative: impl AsRef<Path>) -> bool {
    path(relative).is_ok_and(|p| p.exists())
}

/// Write bytes into the secrets tree at mode 0600, creating parents.
///
/// The mode is applied at `open` time rather than afterwards, so the content
/// is never briefly world-readable between creation and `chmod`.
pub fn write(relative: impl AsRef<Path>, bytes: &[u8]) -> Result<PathBuf> {
    let p = path(relative)?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&p)
        .with_context(|| format!("creating {}", p.display()))?;
    f.write_all(bytes)
        .with_context(|| format!("writing {}", p.display()))?;
    // An existing file keeps its old mode through `open`, so restate it.
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 600 {}", p.display()))?;
    Ok(p)
}

/// Write a UTF-8 secret.
pub fn write_string(relative: impl AsRef<Path>, s: &str) -> Result<PathBuf> {
    write(relative, s.as_bytes())
}

/// Record that a bootstrap stage completed.
///
/// Markers are a skip *hint* only — every stage resolves its objects by natural
/// key before creating them, so a lost marker costs a re-probe, never a
/// duplicate.
pub fn mark_done(stage: &str) -> Result<()> {
    write_string(format!("state/{stage}.done"), "")?;
    Ok(())
}

/// True when a stage marker is present.
#[must_use]
pub fn is_done(stage: &str) -> bool {
    exists(format!("state/{stage}.done"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_and_climbing_paths() {
        assert!(path("/etc/passwd").is_err());
        assert!(path("../outside").is_err());
        assert!(path("pki/../../outside").is_err());
        assert!(path("pki/root.pem").is_ok());
    }
}
