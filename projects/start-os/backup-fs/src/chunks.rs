//! Name indexes of format-2 chunked directories.
//!
//! A chunked directory stores its entries in directory-bucket records used as
//! append-only chunks: a new name joins the last chunk until it is full, so
//! adding entries rewrites only that chunk. Each mounted directory's names are
//! indexed in memory on first use; modified chunks are staged at commit.

use std::collections::{BTreeMap, HashMap};
use std::ffi::{OsStr, OsString};

use crate::ctrl::Controller;
use crate::directory::{Bucket, DirectoryEntry};
use crate::error::BkfsResult;
use crate::inode::Inode;

/// Estimated encoded bytes a chunk holds before a new name opens the next one.
const CHUNK_BYTES: u32 = 56 * 1024;
/// Indexed entries kept across directories before the least recently used are dropped.
const CACHED_ENTRIES: usize = 1 << 18;

fn entry_bytes(name: &OsStr) -> u32 {
    name.len() as u32 + 16
}

pub(crate) struct Chunks {
    generation: u64,
    names: HashMap<OsString, (DirectoryEntry, u32)>,
    /// Estimated encoded bytes of each chunk.
    sizes: Vec<u32>,
    /// Chunks changed since they were last staged; empty ones are removed.
    dirty: BTreeMap<u32, Bucket>,
    used: u64,
}

impl Chunks {
    fn load(ctrl: &Controller, dir: Inode, generation: u64, count: u32) -> BkfsResult<Self> {
        let mut chunks = Self {
            generation,
            names: HashMap::new(),
            sizes: vec![0; count as usize],
            dirty: BTreeMap::new(),
            used: 0,
        };
        for index in 0..count {
            for (name, entry) in ctrl.load_dir_bucket(dir, generation, index)?.iter() {
                chunks.sizes[index as usize] += entry_bytes(name);
                chunks.names.insert(name.clone(), (entry.clone(), index));
            }
        }
        Ok(chunks)
    }

    /// Packs the entries into new, unstaged chunks.
    fn pack(generation: u64, entries: &Bucket) -> Self {
        let mut chunks = Self {
            generation,
            names: HashMap::new(),
            sizes: Vec::new(),
            dirty: BTreeMap::new(),
            used: 0,
        };
        for (name, entry) in entries.iter() {
            chunks.append(name.clone(), entry.clone());
        }
        chunks
    }

    pub fn count(&self) -> u32 {
        self.sizes.len() as u32
    }

    pub fn len(&self) -> u64 {
        self.names.len() as u64
    }

    pub fn subdirs(&self) -> u64 {
        self.names
            .values()
            .filter(|(entry, _)| entry.is_dir())
            .count() as u64
    }

    pub fn get(&self, name: &OsStr) -> Option<&DirectoryEntry> {
        self.names.get(name).map(|(entry, _)| entry)
    }

    pub fn snapshot(&self) -> Bucket {
        self.names
            .iter()
            .map(|(name, (entry, _))| (name.clone(), entry.clone()))
            .collect()
    }

    fn chunk(&mut self, ctrl: &Controller, dir: Inode, index: u32) -> BkfsResult<&mut Bucket> {
        if !self.dirty.contains_key(&index) {
            let stored = ctrl.load_dir_bucket(dir, self.generation, index)?;
            self.dirty.insert(index, stored);
        }
        Ok(self.dirty.get_mut(&index).unwrap())
    }

    /// Whether a name of this encoded size joins the last chunk.
    fn fits_last(&self, bytes: u32) -> bool {
        self.sizes
            .last()
            .is_some_and(|&used| used == 0 || used + bytes <= CHUNK_BYTES)
    }

    /// Adds a name absent from every chunk to the last one, or to a new one.
    /// The last chunk must already be loaded when the name fits it.
    fn append(&mut self, name: OsString, entry: DirectoryEntry) {
        let bytes = entry_bytes(&name);
        if !self.fits_last(bytes) {
            self.sizes.push(0);
            self.dirty.insert(self.count() - 1, Bucket::new());
        }
        let index = self.count() - 1;
        self.sizes[index as usize] += bytes;
        self.dirty
            .get_mut(&index)
            .expect("the last chunk is loaded before appending")
            .insert(name.clone(), entry.clone());
        self.names.insert(name, (entry, index));
    }

    /// Returns the entry the name replaced.
    pub fn insert(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        name: OsString,
        entry: DirectoryEntry,
    ) -> BkfsResult<Option<DirectoryEntry>> {
        if let Some((previous, index)) = self.names.get(&name).cloned() {
            self.chunk(ctrl, dir, index)?
                .insert(name.clone(), entry.clone());
            self.names.insert(name, (entry, index));
            return Ok(Some(previous));
        }
        if self.fits_last(entry_bytes(&name)) {
            self.chunk(ctrl, dir, self.count() - 1)?;
        }
        self.append(name, entry);
        Ok(None)
    }

    pub fn remove(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        name: &OsStr,
    ) -> BkfsResult<Option<DirectoryEntry>> {
        let Some((entry, index)) = self.names.remove(name) else {
            return Ok(None);
        };
        self.chunk(ctrl, dir, index)?.remove(name);
        self.sizes[index as usize] -= entry_bytes(name);
        while self.sizes.last() == Some(&0) {
            self.sizes.pop();
            self.dirty.insert(self.count(), Bucket::new());
        }
        let live: u64 = self.sizes.iter().map(|&bytes| bytes as u64).sum();
        if self.count() > 4 && live * 4 < self.count() as u64 * CHUNK_BYTES as u64 {
            self.repack();
        }
        Ok(Some(entry))
    }

    /// Chunk slots that may hold stored or staged records.
    fn slots(&self) -> u32 {
        self.dirty
            .keys()
            .next_back()
            .map_or(0, |&index| index + 1)
            .max(self.count())
    }

    /// Rewrites the entries densely and removes the chunks left over.
    fn repack(&mut self) {
        let mut packed = Self::pack(self.generation, &self.snapshot());
        for index in packed.count()..self.slots() {
            packed.dirty.insert(index, Bucket::new());
        }
        packed.used = self.used;
        *self = packed;
    }

    /// Stages the changed chunks.
    fn stage(&mut self, ctrl: &Controller, dir: Inode) -> BkfsResult<()> {
        for (index, chunk) in std::mem::take(&mut self.dirty) {
            ctrl.save_dir_bucket(dir, self.generation, index, &chunk)?;
        }
        Ok(())
    }
}

pub(crate) struct ChunkCache {
    directories: HashMap<u64, Chunks>,
    clock: u64,
    limit: usize,
}

impl Default for ChunkCache {
    fn default() -> Self {
        Self {
            directories: HashMap::new(),
            clock: 0,
            limit: CACHED_ENTRIES,
        }
    }
}

impl ChunkCache {
    /// The indexed chunks of a directory, loaded on first use.
    pub fn open(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        generation: u64,
        count: u32,
    ) -> BkfsResult<&mut Chunks> {
        if self
            .directories
            .get(&dir.0)
            .is_some_and(|chunks| chunks.generation != generation)
        {
            self.directories.remove(&dir.0);
        }
        if !self.directories.contains_key(&dir.0) {
            let chunks = Chunks::load(ctrl, dir, generation, count)?;
            self.directories.insert(dir.0, chunks);
            self.evict(ctrl, dir)?;
        }
        self.clock += 1;
        let chunks = self.directories.get_mut(&dir.0).unwrap();
        chunks.used = self.clock;
        Ok(chunks)
    }

    /// Replaces the directory's chunks with a dense layout of the entries.
    pub fn create(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        generation: u64,
        entries: &Bucket,
    ) -> BkfsResult<&mut Chunks> {
        self.clock += 1;
        let mut chunks = Chunks::pack(generation, entries);
        chunks.used = self.clock;
        self.directories.insert(dir.0, chunks);
        self.evict(ctrl, dir)?;
        Ok(self.directories.get_mut(&dir.0).unwrap())
    }

    /// Drops the directory's index and unstaged chunks, returning how many
    /// chunk slots may hold records.
    pub fn forget(&mut self, dir: Inode) -> Option<u32> {
        self.directories.remove(&dir.0).map(|chunks| chunks.slots())
    }

    #[cfg(test)]
    pub(crate) fn set_limit(&mut self, entries: usize) {
        self.limit = entries;
    }

    /// Stages every changed chunk for the next commit.
    pub fn stage(&mut self, ctrl: &Controller) -> BkfsResult<()> {
        for (&dir, chunks) in &mut self.directories {
            chunks.stage(ctrl, Inode(dir))?;
        }
        Ok(())
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.directories
            .values()
            .map(|chunks| chunks.names.len())
            .sum()
    }

    pub(crate) fn evict(&mut self, ctrl: &Controller, keep: Inode) -> BkfsResult<()> {
        let mut entries = self.entry_count();
        while entries > self.limit {
            let Some(victim) = self
                .directories
                .iter()
                .filter(|(&dir, _)| dir != keep.0)
                .min_by_key(|(_, chunks)| chunks.used)
                .map(|(&dir, _)| dir)
            else {
                break;
            };
            let mut chunks = self.directories.remove(&victim).unwrap();
            chunks.stage(ctrl, Inode(victim))?;
            entries -= chunks.names.len();
        }
        Ok(())
    }
}
