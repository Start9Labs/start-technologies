extern crate self as uciedit;

use chrono::DateTime;
use chrono::Utc;
use serde::Serializer;
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;
use std::ops;
use std::path::Path;
use std::path::PathBuf;
use std::str::Utf8Error;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub mod openwrt;

mod syntax;
pub use syntax::*;

pub trait TypedSection<'a>: Sized {
    fn is_type(ty: &str) -> bool;
    fn read(section: &Section<'a>) -> Result<Self, Error>;
    fn write(&self, section: &mut Section<'a>) -> Result<(), Error>;
    fn append(&self, arena: &'a Arena, name: Option<&'a str>) -> Result<Section<'a>, Error>;
}
pub use uciedit_macros::TypedSection;

#[derive(Debug, serde::Serialize)]
pub enum Source {
    Unknown,
    UnknownLine(usize),
    Line(PathBuf, usize),
    Path(PathBuf),
}

impl From<usize> for Source {
    fn from(value: usize) -> Self {
        Source::UnknownLine(value)
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Unknown => write!(f, "???"),
            Source::UnknownLine(line) => write!(f, "???:{}", line),
            Source::Line(path, line) => write!(f, "{}:{}", path.display(), line),
            Source::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

fn ser_std_err<S: Serializer>(err: &impl std::error::Error, ser: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&err.to_string(), ser)
}

#[derive(serde::Serialize, thiserror::Error, Debug)]
#[serde(tag = "what")]
pub enum Error {
    #[error("{cause} at {src}")]
    Io {
        #[serde(serialize_with = "ser_std_err")]
        cause: std::io::Error,
        src: Source,
    },
    #[error("{cause} at {src}")]
    Utf8 {
        #[serde(serialize_with = "ser_std_err")]
        cause: Utf8Error,
        src: Source,
    },
    #[error("config {src} was modified more recently")]
    Conflict { src: Source },
    #[error("{cause} at {src}")]
    FdLock {
        #[serde(serialize_with = "ser_std_err")]
        cause: fd_lock_rs::Error,
        src: Source,
    },
    #[error("bad section type at {src}: {desc}")]
    BadSection { src: Source, desc: String },
    #[error("bad option at {src}: {desc}")]
    BadOption { src: Source, desc: String },
    #[error("bad list at {src}: {desc}")]
    BadList { src: Source, desc: String },
    #[error("unknown uci keyword {found:?} at {src}")]
    UnknownKeyword { src: Source, found: String },
    #[error("expected uci section at {src}")]
    ExpectedSection { src: Source },
    #[error("expected {found:?} section at {src} to be {expected:?}")]
    ExpectedSectionType {
        src: Source,
        expected: String,
        found: String,
    },
    #[error("error parsing a value at {src}: {desc:?}")]
    ValueMsg { src: Source, desc: String },
    #[error("error parsing a value at {src}: {found:?} should be a boolean")]
    ValueBoolean { src: Source, found: String },
    #[error("missing option {missing:?} at {src}")]
    MissingOption { src: Source, missing: String },
}

impl Error {
    pub fn map_src(self, with: impl FnOnce(Source) -> Source) -> Self {
        use Error::*;
        match self {
            Io { cause, src } => Io {
                cause,
                src: with(src),
            },
            Utf8 { cause, src } => Utf8 {
                cause,
                src: with(src),
            },
            Conflict { src } => Conflict { src: with(src) },
            FdLock { cause, src } => FdLock {
                cause,
                src: with(src),
            },
            BadSection { src, desc } => BadSection {
                src: with(src),
                desc,
            },
            BadOption { src, desc } => BadOption {
                src: with(src),
                desc,
            },
            BadList { src, desc } => BadList {
                src: with(src),
                desc,
            },
            UnknownKeyword { src, found } => UnknownKeyword {
                src: with(src),
                found,
            },
            ExpectedSection { src } => ExpectedSection { src: with(src) },
            ExpectedSectionType {
                src,
                expected,
                found,
            } => ExpectedSectionType {
                src: with(src),
                expected,
                found,
            },
            ValueMsg { src, desc } => ValueMsg {
                src: with(src),
                desc,
            },
            ValueBoolean { src, found } => ValueBoolean {
                src: with(src),
                found,
            },
            MissingOption { src, missing } => MissingOption {
                src: with(src),
                missing,
            },
        }
    }
}

trait ResultExt {
    fn with_path(self, path: &Path) -> Self;
}

impl<T> ResultExt for Result<T, Error> {
    fn with_path(self, path: &Path) -> Self {
        use Source::*;
        match self {
            Ok(x) => Ok(x),
            Err(e) => Err(e.map_src(|src| match src {
                Unknown => Path(path.to_path_buf()),
                UnknownLine(n) => Line(path.to_path_buf(), n),
                _ => src,
            })),
        }
    }
}

impl Error {
    pub fn parse(
        error: impl std::error::Error + Sync + Send + 'static,
        line_number: usize,
    ) -> Self {
        Error::ValueMsg {
            src: Source::UnknownLine(line_number),
            desc: error.to_string(),
        }
    }
}

pub struct LockedConfig {
    path: PathBuf,
    locked: fd_lock_rs::FdLock<tokio::fs::File>,
}

impl LockedConfig {
    pub async fn open(path: PathBuf) -> Result<Self, Error> {
        use fd_lock_rs::{FdLock, LockType};
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(false)
            .open(&path)
            .await
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(path.clone()),
            })?;
        // flock is a fast syscall; run it on the blocking pool to avoid
        // stalling the async runtime on contended files.
        let locked = tokio::task::spawn_blocking(move || {
            FdLock::lock(file, LockType::Exclusive, true)
        })
        .await
        .map_err(|e| Error::Io {
            cause: std::io::Error::other(format!("flock task panicked: {e}")),
            src: Source::Path(path.clone()),
        })?
        .map_err(|cause| Error::FdLock {
            cause,
            src: Source::Path(path.clone()),
        })?;
        Ok(LockedConfig { path, locked })
    }

    pub async fn check_modified(&mut self, expected: DateTime<Utc>) -> Result<(), Error> {
        let found = self.get_modified().await.ok();
        if found.is_some_and(|found| found > expected) {
            Err(Error::Conflict {
                src: Source::Path(self.path.clone()),
            })
        } else {
            Ok(())
        }
    }

    pub async fn get_modified(&mut self) -> Result<DateTime<Utc>, Error> {
        let meta = self.locked.metadata().await.map_err(|cause| Error::Io {
            cause,
            src: Source::Path(self.path.clone()),
        })?;
        meta.modified()
            .map(DateTime::<Utc>::from)
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(self.path.clone()),
            })
    }

    pub async fn parse<'a>(&mut self, arena: &'a Arena) -> Result<Config<'a>, Error> {
        let locked = &mut *self.locked;
        locked
            .seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(self.path.clone()),
            })?;
        let mut text = String::new();
        locked
            .read_to_string(&mut text)
            .await
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(self.path.clone()),
            })?;
        let text = arena.alloc(text);
        Config::parse_str(arena, text).with_path(&self.path)
    }

    pub async fn dump(&mut self, config: &Config<'_>) -> Result<(), Error> {
        let text = config.dump_str();
        self.dump_bytes(text.as_bytes()).await
    }

    pub async fn dump_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let tmp_path = self.path.with_extension("tmp");
        let mut tmp_file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(self.path.clone()),
            })?;
        tmp_file
            .write_all(bytes)
            .await
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(self.path.clone()),
            })?;
        tmp_file.flush().await.map_err(|cause| Error::Io {
            cause,
            src: Source::Path(self.path.clone()),
        })?;
        tmp_file.sync_all().await.map_err(|cause| Error::Io {
            cause,
            src: Source::Path(self.path.clone()),
        })?;
        drop(tmp_file);
        tokio::fs::rename(&tmp_path, &self.path)
            .await
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Path(self.path.clone()),
            })?;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Configs<'a> {
    map: BTreeMap<String, Config<'a>>,
}

impl<'a> Configs<'a> {
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Config<'a>)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl<'a> ops::Index<&str> for Configs<'a> {
    type Output = Config<'a>;

    fn index(&self, index: &str) -> &Self::Output {
        match self.map.get(index) {
            Some(cfg) => cfg,
            None => panic!("did not parse config {index}"),
        }
    }
}

impl<'a> ops::IndexMut<&str> for Configs<'a> {
    fn index_mut(&mut self, index: &str) -> &mut Self::Output {
        match self.map.get_mut(index) {
            Some(cfg) => cfg,
            None => panic!("did not parse config {index}"),
        }
    }
}

/// Raw file contents from a retry-safe read of multiple config files.
///
/// This type holds the text bytes and mtimes without any lifetime ties to an
/// Arena. It is `Send`, so it can cross `.await` points freely. Use
/// `ConfigBytes::parse` to convert it into a `Configs<'a>` with an Arena.
pub struct ConfigBytes {
    entries: Vec<(String, Option<DateTime<Utc>>, String)>,
}

impl ConfigBytes {
    /// Parse the raw bytes into typed `Configs` using the given arena.
    /// This step is purely in-memory (no I/O) and thus sync.
    pub fn parse<'a>(self, arena: &'a Arena) -> Result<Configs<'a>, Error> {
        let mut configs = Configs {
            map: BTreeMap::new(),
        };
        for (name, modified, text) in self.entries {
            let text = arena.alloc(text);
            let mut cfg = Config::parse_str(arena, text)
                .map_err(|e| e.map_src(|_| Source::Path(Path::new(&name).to_path_buf())))?;
            cfg.modified = modified;
            configs.map.insert(name, cfg);
        }
        Ok(configs)
    }
}

/// Asynchronously read a set of config files (with retry on concurrent modification).
///
/// Returns raw bytes + mtimes as a `Send` value. Call `.parse(&arena)` on the
/// result to get a typed `Configs<'a>`. This separation is what lets handlers
/// keep their futures `Send`: the arena never crosses an `.await` point.
pub async fn read_all<N: Borrow<str>>(
    root: impl AsRef<Path>,
    names: &[N],
) -> Result<ConfigBytes, Error> {
    const MAX_RETRIES: usize = 4;
    let root = root.as_ref();
    let mut entries: Vec<(String, Option<DateTime<Utc>>, String)> = Vec::with_capacity(names.len());
    'retry: for _ in 0..MAX_RETRIES {
        entries.clear();
        let expected: DateTime<Utc> = std::time::SystemTime::now().into();
        for name in names {
            let name = name.borrow();
            let path = root.join(name);
            let (modified, text) = match tokio::fs::metadata(&path).await {
                Ok(meta) => {
                    let modified = meta.modified().map(DateTime::<Utc>::from).ok();
                    let text = tokio::fs::read_to_string(&path).await.map_err(|cause| Error::Io {
                        cause,
                        src: Source::Path(path.clone()),
                    })?;
                    (modified, text)
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => (None, String::new()),
                Err(cause) => {
                    return Err(Error::Io {
                        cause,
                        src: Source::Path(path.clone()),
                    })
                }
            };
            entries.push((name.to_string(), modified, text));
        }
        for (_, modified, _) in &entries {
            if let Some(found) = modified {
                if *found > expected {
                    // file was updated after we started reading, retry!
                    continue 'retry;
                }
            }
        }
        break 'retry;
    }
    Ok(ConfigBytes { entries })
}

/// Convenience: parse a set of config files into `Configs<'a>` in a single call.
///
/// NOTE: The returned future is `!Send` because it holds the `&Arena` across
/// `.await` points. Handlers that need `Send` futures should call
/// `read_all(...).await` (which IS `Send`) and then synchronously call
/// `.parse(&arena)` on the result inside a scope that does not cross an `.await`.
pub async fn parse_all<'a>(
    root: impl AsRef<Path>,
    arena: &'a Arena,
    names: &[impl Borrow<str>],
) -> Result<Configs<'a>, Error> {
    read_all(root, names).await?.parse(arena)
}

/// Raw serialized configs produced from `Configs::freeze()`.
///
/// This type is `Send` — it contains only `String`s. Use `write_all` to
/// atomically persist it to disk.
pub struct FrozenConfigs {
    entries: Vec<(String, Option<DateTime<Utc>>, String)>,
}

impl<'a> Configs<'a> {
    /// Serialize all configs to strings so they can be persisted without
    /// holding any arena borrows across `.await` points.
    pub fn freeze(self) -> FrozenConfigs {
        let entries = self
            .map
            .into_iter()
            .map(|(name, config)| {
                let modified = config.modified;
                let text = config.dump_str();
                (name, modified, text)
            })
            .collect();
        FrozenConfigs { entries }
    }
}

/// Asynchronously dump pre-serialized configs atomically (temp + rename),
/// with mtime conflict detection via file locks.
pub async fn write_all(root: impl AsRef<Path>, frozen: FrozenConfigs) -> Result<(), Error> {
    let root = root.as_ref();
    // Lock all the files at once, in lexicographic order (they're already
    // sorted by BTreeMap) so deadlocks are impossible.
    let mut files = Vec::with_capacity(frozen.entries.len());
    for (name, modified, text) in frozen.entries.into_iter() {
        let path = root.join(&name);
        let locked = LockedConfig::open(path).await?;
        files.push((name, modified, text, locked));
    }

    // Check all the modified times.
    for (_, modified, _, file) in &mut files {
        if let Some(expected) = *modified {
            file.check_modified(expected).await?;
        }
    }

    // Actually write the sections.
    for (_, _, text, mut saved) in files {
        saved.dump_bytes(text.as_bytes()).await?;
    }

    Ok(())
}

/// Convenience: dump `Configs<'a>` in a single call. The returned future is
/// `!Send` because it holds the arena across `.await`. Handlers that need
/// `Send` futures should call `configs.freeze()` (sync) to drop the arena
/// borrow, then `write_all(root, frozen).await`.
pub async fn dump_all(root: impl AsRef<Path>, configs: Configs<'_>) -> Result<(), Error> {
    write_all(root, configs.freeze()).await
}

#[cfg(test)]
mod tests;
