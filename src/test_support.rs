//! Test-only filesystem helpers.
//!
//! A temp directory that outlives its test is a leak the owner can see: the
//! suite runs on their machine, so every run would add a directory to /tmp.

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A temp directory removed when its test ends, however the test ends.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(kind: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("{kind}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
