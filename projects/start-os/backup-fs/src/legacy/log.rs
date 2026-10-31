use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use chacha20::Key;

use super::{Checkpoint, Location, Source};
use crate::atomic_file::AtomicFile;
use crate::error::{BkfsError, BkfsErrorKind, BkfsResult, BkfsResultExt};
use crate::seglog::{self, Frames, Record, Roots, SegMeta};
use crate::serde::{data_config, decode, encode};
use crate::state::Counters;
use crate::vault::{self, Binding, EccParams};

pub(crate) struct Log {
    directory: File,
    pub path: PathBuf,
    pub dirty: bool,
    uncertain: bool,
}

impl Log {
    pub fn open(dir: &Path, key: &Key) -> BkfsResult<(Self, Roots, Counters)> {
        std::fs::create_dir_all(dir)?;
        let directory = File::open(dir)?;
        let path = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let mut source = Source::load(&path.join(".."), key)?;
        let mut locations = HashMap::new();
        for (segment, _) in seglog::list_segments(&path)? {
            let bytes = std::fs::read(seglog::segment_path(&path, segment))?;
            for frame in Frames::new(&bytes) {
                let opened = match vault::open_bound(frame.payload, key, Binding::Legacy) {
                    Ok(opened) => opened,
                    Err(error) if matches!(error.kind, BkfsErrorKind::BadChecksum) => continue,
                    Err(error) => return Err(error),
                };
                let record: Record = match decode(&opened.plaintext, data_config()) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
                if matches!(record, Record::Block { .. } | Record::Bucket { .. }) {
                    return Err(BkfsError::unsupported(
                        "unexpected block or bucket record in format 1",
                    ));
                }
                source.observe(
                    &record,
                    frame.seq,
                    Location {
                        segment,
                        offset: frame.offset as u64,
                        len: frame.len,
                    },
                )?;
                locations.insert(
                    (segment, frame.offset as u64),
                    seglog::Location {
                        segment,
                        offset: frame.offset as u64,
                        len: frame.len,
                        seq: frame.seq,
                        payload_len: frame.payload.len() as u32,
                        tag: opened.tag,
                    },
                );
            }
        }
        let roots = source.roots(&locations);
        let counters = Counters {
            next_seq: source.next_seq,
            max_inode: source.max_inode,
        };
        Ok((
            Self {
                directory,
                path,
                dirty: false,
                uncertain: false,
            },
            roots,
            counters,
        ))
    }

    pub fn check_rw(&self) -> BkfsResult<()> {
        if self.uncertain {
            return BkfsResult::errno_notrace(libc::EIO);
        }
        Ok(())
    }

    pub fn sync(&mut self) -> BkfsResult<()> {
        self.check_rw()?;
        if let Err(error) = crate::util::sync_filesystem(&self.directory) {
            self.uncertain = true;
            return Err(error.into());
        }
        self.dirty = false;
        Ok(())
    }

    pub fn checkpoint(
        &mut self,
        key: &Key,
        ecc: EccParams,
        roots: &Roots,
        counters: Counters,
        meta: &HashMap<u64, SegMeta>,
    ) -> BkfsResult<()> {
        self.sync()?;
        let convert = |index: &HashMap<u64, seglog::Location>| {
            index
                .iter()
                .map(|(&id, loc)| {
                    (
                        id,
                        Location {
                            segment: loc.segment,
                            offset: loc.offset,
                            len: loc.len,
                        },
                    )
                })
                .collect()
        };
        let checkpoint = Checkpoint {
            version: 1,
            next_seq: counters.next_seq,
            max_inode: counters.max_inode,
            index: convert(&roots.index),
            content: convert(&roots.content),
            _seg_meta: meta.clone(),
            segments: seglog::list_segments(&self.path)?,
        };
        let bytes = vault::seal_legacy(&encode(&checkpoint, data_config())?, key, ecc);
        let mut file = AtomicFile::create_buffered(self.path.join(seglog::INDEX_CACHE_NAME))?;
        file.write_all(&bytes)?;
        file.save_durable()
    }
}
