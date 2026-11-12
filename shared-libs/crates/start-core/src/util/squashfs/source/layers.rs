use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
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
use crate::util::squashfs::{DirectoryContents, Entry, Node, NodeContents, Squashfs};

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
        self.apply_entries(entries)?;
        for path in whiteouts {
            self.ensure_directory(path.parent().expect("whiteout target has a filename"))?;
        }
        for path in opaque {
            self.ensure_directory(&path)?;
        }
        Ok(())
    }

    fn apply_entries(
        &mut self,
        entries: impl IntoIterator<Item = (PathBuf, Entry<DynFileSource>)>,
    ) -> std::io::Result<()> {
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
        Ok(())
    }

    pub(crate) fn from_squashfs(image: Squashfs<DynFileSource>) -> Result<Self, Error> {
        let mut filesystem = Self::new();
        filesystem.apply_entries(snapshot_entries(&image))?;
        Ok(filesystem)
    }

    pub(crate) fn apply_overlayfs_upper(
        &mut self,
        upper: Squashfs<DynFileSource>,
    ) -> Result<(), Error> {
        let mut entries = Vec::new();
        let mut links = Vec::new();
        let mut whiteouts = Vec::new();
        let mut opaque = Vec::new();
        for (path, entry) in snapshot_entries(&upper) {
            let Entry::Node(mut node) = entry else {
                let node = resolve_hardlink(&path, |target| {
                    upper.contents().get_path(target).map(|entry| match entry {
                        Entry::Node(node) => Ok(node),
                        Entry::Hardlink(target) => Err(target.clone()),
                    })
                })?;
                if overlay_whiteout(node) {
                    whiteouts.push(path);
                } else {
                    links.push((path, entry));
                }
                continue;
            };
            let whiteout = overlay_whiteout(&node);
            let xattrs = &mut node.metadata.xattrs;
            for name in overlay_names("metacopy").chain(overlay_names("redirect")) {
                if xattrs.contains_key(&name) {
                    return Err(invalid(
                        "OverlayFS snapshot requires metacopy and redirect_dir disabled",
                    )
                    .into());
                }
            }
            let has_whiteout = overlay_names("whiteout").any(|name| xattrs.contains_key(&name));
            if whiteout {
                if path.as_os_str().is_empty() {
                    return Err(invalid("OverlayFS root is a whiteout").into());
                }
                whiteouts.push(path);
                continue;
            }
            if has_whiteout {
                return Err(invalid("OverlayFS whiteout is not a file").into());
            }
            let recreated =
                overlay_names("opaque").any(|name| xattrs.get(&name).is_some_and(|v| v == b"y"));
            if recreated {
                if !matches!(node.contents, NodeContents::Directory(_)) {
                    return Err(invalid("OverlayFS opaque inode is not a directory").into());
                }
                opaque.push(path.clone());
            }
            xattrs.retain(|name, _| {
                !OVERLAY_PREFIXES
                    .iter()
                    .any(|prefix| name.as_bytes().starts_with(prefix.as_bytes()))
            });
            if !opaque.iter().any(|ancestor| path.starts_with(ancestor))
                && matches!(node.contents, NodeContents::Directory(_))
            {
                if let Some(lower) = self
                    .nodes
                    .get(&path)
                    .filter(|node| matches!(node.contents, NodeContents::Directory(_)))
                {
                    for name in overlay_names("opaque") {
                        if let Some(value) = lower.metadata.xattrs.get(&name) {
                            xattrs.insert(name, value.clone());
                        }
                    }
                }
            }
            entries.push((path, Entry::Node(node)));
        }
        for path in whiteouts {
            self.remove_subtree(&path);
        }
        for path in opaque {
            self.remove_descendants(&path);
        }
        entries.extend(links);
        self.apply_entries(entries)?;
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

const OVERLAY_PREFIXES: [&str; 2] = ["trusted.overlay.", "user.overlay."];

fn overlay_names(suffix: &str) -> impl Iterator<Item = OsString> + '_ {
    OVERLAY_PREFIXES
        .iter()
        .map(move |prefix| format!("{prefix}{suffix}").into())
}

fn overlay_whiteout(node: &Node<DynFileSource>) -> bool {
    matches!(
        node.contents,
        NodeContents::CharacterDevice(crate::util::squashfs::Device { major: 0, minor: 0 })
    ) || (matches!(node.contents, NodeContents::File(_))
        && overlay_names("whiteout").any(|name| node.metadata.xattrs.contains_key(&name)))
}

fn snapshot_entries(image: &Squashfs<DynFileSource>) -> Vec<(PathBuf, Entry<DynFileSource>)> {
    let mut entries = Vec::new();
    let mut pending = vec![(PathBuf::new(), image.root())];
    while let Some((path, node)) = pending.pop() {
        if let NodeContents::Directory(children) = &node.contents {
            entries.push((
                path.clone(),
                Entry::directory(node.metadata.clone(), DirectoryContents::new()),
            ));
            for (name, entry) in children.iter() {
                let path = path.join(name);
                match entry {
                    Entry::Node(node) => pending.push((path, node)),
                    Entry::Hardlink(target) => {
                        entries.push((path, Entry::Hardlink(target.clone())))
                    }
                }
            }
        } else {
            entries.push((path, Entry::Node(node.clone())));
        }
    }
    entries
}

#[cfg(test)]
mod tests;
