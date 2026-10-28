use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::AsyncRead;

use super::insert;
use super::tar::TarReader;
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::{DynFileSource, FileSource};
use crate::util::squashfs::blocks::invalid;
use crate::util::squashfs::tree::resolve_hardlink;
use crate::util::squashfs::{Entry, Node, NodeContents, Squashfs};

pub(crate) struct LayerFilesystem {
    nodes: BTreeMap<PathBuf, Arc<Node<DynFileSource>>>,
}

impl LayerFilesystem {
    pub(crate) fn new() -> Self {
        Self {
            nodes: BTreeMap::from([(PathBuf::new(), Arc::new(Node::implicit_directory()))]),
        }
    }

    fn remove_descendants(&mut self, path: &Path) {
        loop {
            let next = self
                .nodes
                .range::<Path, _>((Bound::Excluded(path), Bound::Unbounded))
                .next()
                .filter(|(candidate, _)| candidate.starts_with(path))
                .map(|(candidate, _)| candidate.clone());
            let Some(next) = next else { break };
            self.nodes.remove(&next);
        }
    }

    fn remove_subtree(&mut self, path: &Path) {
        self.nodes.remove(path);
        self.remove_descendants(path);
    }

    fn ensure_directory(&mut self, path: &Path) -> std::io::Result<()> {
        for parent in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            match self.nodes.get(parent) {
                Some(node) if !matches!(node.contents, NodeContents::Directory(_)) => {
                    return Err(invalid("image layer ancestor is not a directory"));
                }
                Some(_) => {}
                None => {
                    self.nodes
                        .insert(parent.to_owned(), Arc::new(Node::implicit_directory()));
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn apply_tar<R: AsyncRead + Unpin + Send>(
        &mut self,
        reader: R,
    ) -> Result<(), Error> {
        let mut reader = TarReader::new(reader).await?;
        let mut entries = Vec::new();
        let mut paths = BTreeSet::new();
        let mut whiteouts = Vec::new();
        let mut opaque = Vec::new();
        while let Some((path, entry)) = reader.next().await? {
            if !paths.insert(path.clone()) {
                return Err(invalid("duplicate image layer path").into());
            }
            if let Some(name) = path
                .file_name()
                .and_then(|name| name.as_bytes().strip_prefix(b".wh."))
            {
                let Entry::Node(Node {
                    contents: NodeContents::File(source),
                    ..
                }) = entry
                else {
                    return Err(invalid("image whiteout is not a regular file").into());
                };
                if source.size().await? != 0 {
                    return Err(invalid("image whiteout is not empty").into());
                }
                let parent = path.parent().expect("whiteout has a filename");
                if name == b".wh..opq" {
                    opaque.push(parent.to_owned());
                } else {
                    super::super::tree::validate_name(std::ffi::OsStr::from_bytes(name))?;
                    whiteouts.push(parent.join(std::ffi::OsString::from_vec(name.to_vec())));
                }
            } else {
                entries.push((path, entry.into_dyn()));
            }
        }
        for path in &whiteouts {
            self.remove_subtree(path);
        }
        for path in &opaque {
            self.remove_descendants(path);
        }
        let mut pending = BTreeMap::new();
        for (path, entry) in entries {
            if let Some(parent) = path.parent() {
                self.ensure_directory(parent)?;
            }
            match entry {
                Entry::Node(node) => {
                    if path.as_os_str().is_empty()
                        && !matches!(node.contents, NodeContents::Directory(_))
                    {
                        return Err(invalid("image root is not a directory").into());
                    }
                    if !matches!(node.contents, NodeContents::Directory(_)) {
                        self.remove_descendants(&path);
                    }
                    self.nodes.insert(path, Arc::new(node));
                }
                Entry::Hardlink(target) => {
                    if path.as_os_str().is_empty() {
                        return Err(invalid("image root cannot be a hardlink").into());
                    }
                    self.remove_subtree(&path);
                    if let Some(node) = self.nodes.get(&target) {
                        node.validate_hardlink_target()?;
                        self.nodes.insert(path, node.clone());
                    } else {
                        pending.insert(path, target);
                    }
                }
            }
        }
        for path in pending.keys() {
            let node = resolve_hardlink(path, |target| {
                self.nodes
                    .get(target)
                    .cloned()
                    .map(Ok)
                    .or_else(|| pending.get(target).cloned().map(Err))
            })?;
            node.validate_hardlink_target()?;
            self.nodes.insert(path.clone(), node);
        }
        for path in whiteouts {
            self.ensure_directory(path.parent().expect("whiteout target has a filename"))?;
        }
        for path in opaque {
            self.ensure_directory(&path)?;
        }
        Ok(())
    }

    pub(crate) fn into_squashfs(self) -> Result<Squashfs<DynFileSource>, Error> {
        let mut owners: BTreeMap<usize, PathBuf> = BTreeMap::new();
        let mut image = Squashfs::default();
        for (path, node) in self.nodes {
            let identity = Arc::as_ptr(&node) as usize;
            let entry = match owners.get(&identity) {
                Some(owner) => Entry::Hardlink(owner.clone()),
                None => {
                    owners.insert(identity, path.clone());
                    Entry::Node((*node).clone())
                }
            };
            insert(&mut image, path, entry)?;
        }
        Ok(image)
    }
}

#[cfg(test)]
mod tests;
