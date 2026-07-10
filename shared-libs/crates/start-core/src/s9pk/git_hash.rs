use std::ops::Deref;
use std::path::Path;

use tokio::process::Command;
use ts_rs::TS;

use crate::prelude::*;
use crate::util::Invoke;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, TS, PartialEq, Eq)]
#[ts(type = "string")]
pub struct GitHash(String);

impl GitHash {
    /// The commit hash at `path` (suffixed `-modified` if the tree is dirty), or `None`
    /// when there's nothing to read: not a git repo, no commits yet (a fresh
    /// `git init`), or no `git` on PATH. Omitting the hash lets a just-scaffolded
    /// package build before its first commit; it populates once one exists.
    pub async fn from_path(path: impl AsRef<Path>) -> Result<Option<GitHash>, Error> {
        let Ok(rev) = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&path)
            .invoke(ErrorKind::Git)
            .await
        else {
            return Ok(None);
        };
        let mut hash = String::from_utf8(rev)?;
        while hash.ends_with(|c: char| c.is_whitespace()) {
            hash.pop();
        }
        // git status (not diff-index): stat-only diff-index misreads a
        // touched-but-unchanged tracked file as dirty. status compares content.
        if !Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .arg("--untracked-files=no")
            .current_dir(&path)
            .invoke(ErrorKind::Git)
            .await?
            .is_empty()
        {
            hash += "-modified";
        }
        Ok(Some(GitHash(hash)))
    }
    pub fn load_sync() -> Option<GitHash> {
        let mut hash = String::from_utf8(
            std::process::Command::new("git")
                .arg("rev-parse")
                .arg("HEAD")
                .output()
                .ok()?
                .stdout,
        )
        .ok()?;
        while hash.ends_with(|c: char| c.is_whitespace()) {
            hash.pop();
        }
        let status = std::process::Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .arg("--untracked-files=no")
            .output()
            .ok()?;
        if !status.status.success() || !status.stdout.is_empty() {
            hash += "-modified";
        }

        Some(GitHash(hash))
    }
}

impl AsRef<str> for GitHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for GitHash {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn tmp() -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "git-hash-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn none_without_a_commit() {
        // not a git repo → no hash, no error
        let dir = tmp();
        assert!(GitHash::from_path(&dir).await.unwrap().is_none());

        // `git init` with no commit yet → still no hash (this is the fresh-scaffold case)
        Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&dir)
            .invoke(ErrorKind::Git)
            .await
            .unwrap();
        assert!(GitHash::from_path(&dir).await.unwrap().is_none());
    }
}
