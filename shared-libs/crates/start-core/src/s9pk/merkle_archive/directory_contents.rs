use std::ffi::OsStr;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use blake3::Hash;
use futures::FutureExt;
use futures::future::BoxFuture;
use imbl::OrdMap;
use imbl_value::InternedString;
use itertools::Itertools;
use tokio::io::AsyncRead;

use crate::CAP_10_MiB;
use crate::prelude::*;
use crate::s9pk::merkle_archive::sink::Sink;
use crate::s9pk::merkle_archive::source::{ArchiveSource, DynFileSource, FileSource, Section};
use crate::s9pk::merkle_archive::write_queue::WriteQueue;
use crate::s9pk::merkle_archive::{Entry, EntryContents, varint};
use crate::util::io::{ParallelBlake3Writer, TrackingIO};

#[derive(Clone)]
pub struct DirectoryContents<S> {
    contents: OrdMap<InternedString, Entry<S>>,
    source_end: u64,
    sort_by: Option<Arc<dyn Fn(&str, &str) -> std::cmp::Ordering + Send + Sync>>,
}
impl<S: Debug> Debug for DirectoryContents<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryContents")
            .field("contents", &self.contents)
            .finish_non_exhaustive()
    }
}
impl<S> DirectoryContents<S> {
    pub(crate) fn source_end(&self) -> u64 {
        self.source_end
    }
    pub fn new() -> Self {
        Self {
            contents: OrdMap::new(),
            sort_by: None,
            source_end: 0,
        }
    }

    pub fn sort_by(
        &mut self,
        sort_by: impl Fn(&str, &str) -> std::cmp::Ordering + Send + Sync + 'static,
    ) {
        self.sort_by = Some(Arc::new(sort_by))
    }

    #[instrument(skip_all)]
    pub fn get_path(&self, path: impl AsRef<Path>) -> Option<&Entry<S>> {
        let mut dir = Some(self);
        let mut res = None;
        for segment in path.as_ref().into_iter() {
            let segment = segment.to_str()?;
            if segment == "/" {
                continue;
            }
            res = dir?.get(segment);
            if let Some(EntryContents::Directory(d)) = res.as_ref().map(|e| e.as_contents()) {
                dir = Some(d);
            } else {
                dir = None
            }
        }
        res
    }

    pub fn file_paths(&self, prefix: impl AsRef<Path>) -> Vec<PathBuf> {
        let prefix = prefix.as_ref();
        let mut res = Vec::new();
        for (name, entry) in &self.contents {
            let path = prefix.join(name);
            if let EntryContents::Directory(d) = entry.as_contents() {
                res.push(path.join(""));
                res.append(&mut d.file_paths(path));
            } else {
                res.push(path);
            }
        }
        res
    }

    pub const fn header_size() -> u64 {
        8 // position: u64 BE
        + 8 // size: u64 BE
    }

    #[instrument(skip_all)]
    pub async fn serialize_header<W: Sink>(&self, position: u64, w: &mut W) -> Result<u64, Error> {
        use tokio::io::AsyncWriteExt;

        let size = self.toc_size();

        w.write_all(&position.to_be_bytes()).await?;
        w.write_all(&size.to_be_bytes()).await?;

        Ok(position)
    }

    pub fn toc_size(&self) -> u64 {
        self.iter().fold(
            varint::serialized_varint_size(self.len() as u64),
            |acc, (name, entry)| {
                acc + varint::serialized_varstring_size(&**name) + entry.header_size()
            },
        )
    }
}
impl<S: Clone> DirectoryContents<S> {
    pub fn with_stem(&self, stem: &str) -> impl Iterator<Item = (InternedString, Entry<S>)> {
        let prefix = InternedString::intern(stem);
        let (_, center, right) = self.split_lookup(&*stem);
        center.map(|e| (prefix.clone(), e)).into_iter().chain(
            right.into_iter().take_while(move |(k, _)| {
                Path::new(&**k).file_stem() == Some(OsStr::new(&*prefix))
            }),
        )
    }
    pub fn insert_path(&mut self, path: impl AsRef<Path>, entry: Entry<S>) -> Result<(), Error> {
        let path = path.as_ref();
        let (parent, Some(file)) = (path.parent(), path.file_name().and_then(|f| f.to_str()))
        else {
            return Err(Error::new(
                eyre!("cannot create file at root"),
                ErrorKind::Pack,
            ));
        };
        let mut dir = self;
        for segment in parent.into_iter().flatten() {
            let segment = segment
                .to_str()
                .ok_or_else(|| Error::new(eyre!("non-utf8 path segment"), ErrorKind::Utf8))?;
            if segment == "/" {
                continue;
            }
            if !dir.contains_key(segment) {
                dir.insert(
                    segment.into(),
                    Entry::new(EntryContents::Directory(DirectoryContents::new())),
                );
            }
            if let Some(EntryContents::Directory(d)) =
                dir.get_mut(segment).map(|e| e.as_contents_mut())
            {
                dir = d;
            } else {
                return Err(Error::new(
                    eyre!(
                        "failed to insert entry at path {path:?}: ancestor exists and is not a directory"
                    ),
                    ErrorKind::Pack,
                ));
            }
        }
        dir.insert(file.into(), entry);
        Ok(())
    }
}
impl<S: ArchiveSource + Clone> DirectoryContents<Section<S>> {
    #[instrument(skip_all)]
    pub fn deserialize<'a>(
        source: &'a S,
        header: &'a mut (impl AsyncRead + Unpin + Send),
        (sighash, max_size): (Hash, u64),
    ) -> BoxFuture<'a, Result<Self, Error>> {
        async move {
            use tokio::io::AsyncReadExt;

            let mut position = [0u8; 8];
            header.read_exact(&mut position).await?;
            let position = u64::from_be_bytes(position);

            let mut size = [0u8; 8];
            header.read_exact(&mut size).await?;
            let size = u64::from_be_bytes(size);

            ensure_code!(
                size <= max_size,
                ErrorKind::InvalidSignature,
                "size is greater than signed"
            );

            let mut source_end = super::source::checked_end(source, position, size).await?;
            let mut toc_reader = source.fetch(position, size).await?;

            let len = varint::deserialize_varint(&mut toc_reader).await?;
            let mut entries = OrdMap::new();
            for _ in 0..len {
                let name = varint::deserialize_varstring(&mut toc_reader).await?;
                let entry = Entry::deserialize(source.clone(), &mut toc_reader).await?;
                source_end = source_end.max(match entry.as_contents() {
                    EntryContents::Missing => 0,
                    EntryContents::File(f) => f.end(),
                    EntryContents::Directory(d) => d.source_end(),
                });
                entries.insert(name.into(), entry);
            }

            let res = Self {
                contents: entries,
                sort_by: None,
                source_end,
            };

            if res.sighash().await? == sighash {
                Ok(res)
            } else {
                Err(Error::new(
                    eyre!("hash sum does not match"),
                    ErrorKind::InvalidSignature,
                ))
            }
        }
        .boxed()
    }
}
impl<S: FileSource + Clone> DirectoryContents<S> {
    pub fn filter(&mut self, filter: impl Fn(&Path) -> bool) -> Result<(), Error> {
        for k in self.keys().cloned().collect::<Vec<_>>() {
            let path = Path::new(&*k);
            if let Some(v) = self.get_mut(&k) {
                if !filter(path) {
                    if v.hash.is_none() {
                        return Err(Error::new(
                            eyre!(
                                "cannot filter out unhashed file {}, run `update_hashes` first",
                                path.display()
                            ),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    v.contents = EntryContents::Missing;
                } else {
                    let filter: Box<dyn Fn(&Path) -> bool> = Box::new(|p| filter(&path.join(p)));
                    v.filter(filter)?;
                }
            }
        }
        Ok(())
    }
    #[instrument(skip_all)]
    pub fn update_hashes<'a>(&'a mut self, only_missing: bool) -> BoxFuture<'a, Result<(), Error>> {
        async move {
            for key in self.keys().cloned().collect::<Vec<_>>() {
                if let Some(entry) = self.get_mut(&key) {
                    entry.update_hash(only_missing).await?;
                }
            }
            Ok(())
        }
        .boxed()
    }

    #[instrument(skip_all)]
    pub fn sighash<'a>(&'a self) -> BoxFuture<'a, Result<Hash, Error>> {
        async move {
            let mut hasher = TrackingIO::new(0, ParallelBlake3Writer::new(CAP_10_MiB));
            let mut sig_contents = OrdMap::new();
            for (name, entry) in &**self {
                sig_contents.insert(name.clone(), entry.to_missing().await?);
            }
            Self {
                contents: sig_contents,
                sort_by: None,
                source_end: 0,
            }
            .serialize_toc(&mut WriteQueue::new(0), &mut hasher)
            .await?;
            let hash = hasher.into_inner().finalize().await?;
            Ok(hash)
        }
        .boxed()
    }

    #[instrument(skip_all)]
    pub async fn serialize_toc<'a, W: Sink>(
        &'a self,
        queue: &mut WriteQueue<'a, S>,
        w: &mut W,
    ) -> Result<(), Error> {
        varint::serialize_varint(self.len() as u64, w).await?;
        for (name, entry) in self.iter().sorted_by(|a, b| match (a, b, &self.sort_by) {
            ((_, a), (_, b), _) if a.as_contents().is_dir() && !b.as_contents().is_dir() => {
                std::cmp::Ordering::Less
            }
            ((_, a), (_, b), _) if !a.as_contents().is_dir() && b.as_contents().is_dir() => {
                std::cmp::Ordering::Greater
            }
            ((_, a), (_, b), _)
                if a.as_contents().is_missing() && !b.as_contents().is_missing() =>
            {
                std::cmp::Ordering::Greater
            }
            ((_, a), (_, b), _)
                if !a.as_contents().is_missing() && b.as_contents().is_missing() =>
            {
                std::cmp::Ordering::Less
            }
            ((n_a, a), (n_b, b), _)
                if a.as_contents().is_missing() && b.as_contents().is_missing() =>
            {
                n_a.cmp(n_b)
            }
            ((a, _), (b, _), Some(sort_by)) => sort_by(&***a, &***b),
            _ => std::cmp::Ordering::Equal,
        }) {
            varint::serialize_varstring(&**name, w).await?;
            entry.serialize_header(queue.add(entry).await?, w).await?;
        }

        Ok(())
    }

    pub fn into_dyn(self) -> DirectoryContents<DynFileSource> {
        DirectoryContents {
            contents: self
                .contents
                .into_iter()
                .map(|(k, v)| (k, v.into_dyn()))
                .collect(),
            sort_by: self.sort_by,
            source_end: self.source_end,
        }
    }
}
impl<S> std::ops::Deref for DirectoryContents<S> {
    type Target = OrdMap<InternedString, Entry<S>>;
    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}
impl<S> std::ops::DerefMut for DirectoryContents<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.contents
    }
}
