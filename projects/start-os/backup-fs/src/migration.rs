use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chacha20::Key;
use crossbeam_channel::bounded;
use log::warn;

use crate::atomic_file::AtomicFile;
use crate::error::{BkfsErrorKind, BkfsResult};
use crate::references::StorageReferences;
use crate::seglog::{self, Frames, Location, Record, RecordKey, SealedRecord, SegmentLog};
use crate::serde::{data_config, decode};
use crate::state::{Counters, State};
use crate::superblock::Constants;
use crate::vault::{self, AuthenticationTag, Binding, EccParams};
use crate::{legacy, ConversionPhase, ConversionProgress};

const READERS: usize = 16;
const BATCH_BYTES: u64 = 64 << 20;

#[cfg(test)]
thread_local! {
    /// Object-batch commits that fail with ENOSPC on this thread.
    pub(crate) static FULL_COMMITS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn commit_batch(log: &mut SegmentLog) -> BkfsResult<()> {
    #[cfg(test)]
    if FULL_COMMITS.with(|full| full.replace(full.get().saturating_sub(1)) > 0) {
        return Err(std::io::Error::from_raw_os_error(libc::ENOSPC).into());
    }
    log.sync()
}

pub(crate) struct UpgradePlain {
    pub plaintext: Vec<u8>,
    pub current_tag: Option<AuthenticationTag>,
}

/// Requires the exclusive store lock and authenticated pending marker.
pub(crate) fn open_for_upgrade(
    blob: &[u8],
    key: &Key,
    binding: Binding,
) -> BkfsResult<UpgradePlain> {
    match vault::open_bound(blob, key, binding) {
        Ok(opened) => {
            return Ok(UpgradePlain {
                plaintext: opened.plaintext,
                current_tag: Some(opened.tag),
            })
        }
        Err(error) if matches!(error.kind, BkfsErrorKind::BadChecksum) => {}
        Err(error) => return Err(error),
    }
    let plaintext = vault::open_legacy(blob, key)?;
    Ok(UpgradePlain {
        plaintext,
        current_tag: None,
    })
}

/// Unreadable input, reported once per attempt.
#[derive(Default)]
struct Unreadable {
    objects: AtomicU64,
    records: AtomicU64,
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        let objects = *self.objects.get_mut();
        let records = *self.records.get_mut();
        if objects > 0 || records > 0 {
            warn!("conversion encountered {objects} unreadable objects and {records} unreadable records");
        }
    }
}

type Report<'a> = &'a mut dyn FnMut(ConversionProgress) -> BkfsResult<()>;

fn progress(
    report: &mut Report,
    phase: ConversionPhase,
    done: u64,
    total: Option<u64>,
) -> BkfsResult<()> {
    report(ConversionProgress { phase, done, total })
}

/// Requires the exclusive store lock and authenticated pending marker.
pub(crate) fn reseal_store(
    data: &Path,
    key: &Key,
    constants: Constants,
    mut report: Report,
) -> BkfsResult<()> {
    let dir = data.join("segments");
    fs::create_dir_all(&dir)?;
    let unreadable = Unreadable::default();
    if !dir.join("state").try_exists()? {
        reseal_log(data, &dir, key, constants, &mut report, &unreadable)?;
    }
    let mut batch = BATCH_BYTES;
    loop {
        match migrate_objects(data, key, constants, batch, &mut report, &unreadable) {
            Err(error)
                if batch > 1
                    && matches!(&error.kind, BkfsErrorKind::Io(io) if io.raw_os_error() == Some(libc::ENOSPC)) =>
            {
                batch /= 4;
            }
            result => return result,
        }
    }
}

/// Reseals format-1 log frames in place and publishes their index.
fn reseal_log(
    data: &Path,
    dir: &Path,
    key: &Key,
    constants: Constants,
    report: &mut Report,
    unreadable: &Unreadable,
) -> BkfsResult<()> {
    let mut source = legacy::Source::load(data, key)?;
    let mut locations = HashMap::new();
    let mut sizes = HashMap::new();
    let segments = seglog::list_segments(dir)?;
    let total = segments.iter().map(|(_, size)| size).sum();
    let mut done = 0;
    progress(report, ConversionPhase::Records, done, Some(total))?;
    for (segment, size) in segments {
        let path = seglog::segment_path(dir, segment);
        let bytes = fs::read(&path)?;
        let mut rewritten = bytes.clone();
        let mut changed = false;
        for frame in Frames::new(&bytes) {
            let mut location = Location {
                segment,
                offset: frame.offset as u64,
                len: frame.len,
                seq: frame.seq,
                payload_len: frame.payload.len() as u32,
                tag: Default::default(),
            };
            match open_for_upgrade(frame.payload, key, Binding::Record) {
                Ok(opened) => {
                    let record: Record = match decode(&opened.plaintext, data_config()) {
                        Ok(record) => record,
                        Err(_) => {
                            unreadable.records.fetch_add(1, Ordering::Relaxed);
                            locations.insert((segment, frame.offset as u64), location);
                            continue;
                        }
                    };
                    source.observe(
                        &record,
                        frame.seq,
                        legacy::Location {
                            segment,
                            offset: frame.offset as u64,
                            len: frame.len,
                        },
                    )?;
                    let payload = match opened.current_tag {
                        Some(tag) => {
                            location.tag = tag;
                            frame.payload.to_vec()
                        }
                        None => {
                            let mut sealed = vault::seal_bound(
                                &opened.plaintext,
                                key,
                                EccParams {
                                    data: frame.payload[5],
                                    parity: frame.payload[6],
                                },
                                Binding::Record,
                            );
                            location.tag = sealed.tag;
                            // Preserves cached locations across interrupted migration.
                            sealed.bytes.resize(frame.payload.len(), 0);
                            changed = true;
                            sealed.bytes
                        }
                    };
                    let encoded = seglog::encode_frame(frame.seq, &payload);
                    let end = frame.offset + encoded.len();
                    if end > rewritten.len() {
                        rewritten.resize(end, 0);
                        changed = true;
                    }
                    rewritten[frame.offset..end].copy_from_slice(&encoded);
                }
                Err(error) if matches!(error.kind, BkfsErrorKind::BadChecksum) => {
                    unreadable.records.fetch_add(1, Ordering::Relaxed);
                }
                Err(error) => return Err(error),
            }
            locations.insert((segment, frame.offset as u64), location);
        }
        sizes.insert(segment, rewritten.len() as u64);
        if changed {
            let mut file = AtomicFile::create_buffered(path)?;
            file.write_all(&rewritten)?;
            file.save_durable()?;
        }
        done += size;
        progress(report, ConversionPhase::Records, done, Some(total))?;
    }
    let mut roots = source.roots(&locations);
    roots.sizes = sizes;
    let counters = Counters {
        next_seq: source.next_seq,
        max_inode: source.max_inode,
    };
    State::create(dir, key, constants.ecc(), &roots, counters, None)?;
    Ok(())
}

enum Fetched {
    Missing,
    Sealed(SealedRecord),
    Unreadable(Vec<u8>),
}

fn fetch(
    data: &Path,
    key: &Key,
    ecc: EccParams,
    wanted: RecordKey,
    unreadable: &Unreadable,
) -> BkfsResult<(PathBuf, Fetched)> {
    let path = legacy::object_path(data, key, wanted).expect("object key");
    let blob = match fs::read(&path) {
        Ok(blob) => blob,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((path, Fetched::Missing))
        }
        Err(error) => return Err(error.into()),
    };
    let plaintext = match vault::open_legacy(&blob, key) {
        Ok(plaintext) => plaintext,
        Err(error) if matches!(error.kind, BkfsErrorKind::BadChecksum) => {
            unreadable.objects.fetch_add(1, Ordering::Relaxed);
            return Ok((path, Fetched::Unreadable(blob)));
        }
        Err(error) => return Err(error),
    };
    let sealed = match wanted {
        RecordKey::Block(content, index) => {
            seglog::seal_block(key, ecc, content, index, plaintext)?
        }
        RecordKey::Bucket(directory, generation, index) => {
            seglog::seal_bucket(key, ecc, directory, generation, index, plaintext)?
        }
        _ => unreachable!("object key"),
    };
    Ok((path, Fetched::Sealed(sealed)))
}

/// Moves referenced format-1 object files into data segments.
///
/// A committed record retires its source file; a retry resumes from the
/// committed index.
fn migrate_objects(
    data: &Path,
    key: &Key,
    constants: Constants,
    batch: u64,
    report: &mut Report,
    unreadable: &Unreadable,
) -> BkfsResult<()> {
    let ecc = constants.ecc();
    let mut log = SegmentLog::open_sized(
        data.join("segments"),
        *key,
        constants.segment_size,
        ecc,
        false,
    )?;
    let mut wanted = Vec::new();
    let mut retired = Vec::new();
    let inodes: Vec<_> = log
        .roots()
        .index
        .iter()
        .map(|(&id, &loc)| (id, loc))
        .collect();
    let scan_total = Some(inodes.len() as u64);
    let mut scanned = 0;
    progress(report, ConversionPhase::ScanningObjects, 0, scan_total)?;
    log.visit_record_results(
        inodes.iter().map(|(id, loc)| (RecordKey::Inode(*id), loc)),
        |record| {
            if let Ok(Record::Inode { inode, attrs }) = record {
                StorageReferences::for_inode(inode, &attrs).visit(|object| {
                    if log.roots().get(object).is_some() {
                        retired.push(object);
                    } else {
                        wanted.push(object);
                    }
                });
            }
            scanned += 1;
            if scanned % 1024 == 0 {
                progress(
                    report,
                    ConversionPhase::ScanningObjects,
                    scanned,
                    scan_total,
                )?;
            }
            Ok(())
        },
    )?;
    progress(
        report,
        ConversionPhase::ScanningObjects,
        scanned,
        scan_total,
    )?;
    let total = Some(wanted.len() as u64);
    let mut done = 0;
    progress(report, ConversionPhase::Objects, done, total)?;
    let failure = Mutex::new(None);
    std::thread::scope(|scope| -> BkfsResult<()> {
        let (jobs, job_queue) = bounded::<RecordKey>(READERS * 4);
        let (results, fetched) = bounded(READERS * 4);
        let (unlinks, unlink_queue) = bounded::<PathBuf>(READERS * 64);
        for _ in 0..READERS {
            let job_queue = job_queue.clone();
            let results = results.clone();
            scope.spawn(move || {
                while let Ok(object) = job_queue.recv() {
                    if results
                        .send((object, fetch(data, key, ecc, object, unreadable)))
                        .is_err()
                    {
                        break;
                    }
                }
            });
            let unlink_queue = unlink_queue.clone();
            let failure = &failure;
            scope.spawn(move || {
                while let Ok(path) = unlink_queue.recv() {
                    if let Err(error) = fs::remove_file(&path) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            failure.lock().unwrap().get_or_insert(error);
                        }
                    }
                }
            });
        }
        drop(job_queue);
        drop(results);
        let retired_paths: Vec<_> = retired
            .iter()
            .filter_map(|&object| legacy::object_path(data, key, object))
            .collect();
        let feed = scope.spawn(move || {
            for object in wanted {
                if jobs.send(object).is_err() {
                    break;
                }
            }
        });
        for path in retired_paths {
            let _ = unlinks.send(path);
        }
        let mut sources = Vec::new();
        let mut pending = 0;
        let outcome = (|| -> BkfsResult<()> {
            for (object, result) in fetched.iter() {
                let (path, fetched) = result?;
                match fetched {
                    Fetched::Missing => {}
                    Fetched::Sealed(record) => {
                        pending += record.len();
                        log.append(&record)?;
                        sources.push(path);
                    }
                    Fetched::Unreadable(blob) => {
                        pending += blob.len() as u64;
                        log.preserve_unreadable(object, &blob)?;
                        sources.push(path);
                    }
                }
                if pending >= batch {
                    commit_batch(&mut log)?;
                    pending = 0;
                    for path in sources.drain(..) {
                        let _ = unlinks.send(path);
                    }
                }
                done += 1;
                progress(report, ConversionPhase::Objects, done, total)?;
            }
            commit_batch(&mut log)?;
            for path in sources.drain(..) {
                let _ = unlinks.send(path);
            }
            Ok(())
        })();
        drop(fetched);
        drop(unlinks);
        let _ = feed.join();
        outcome
    })?;
    if let Some(error) = failure.into_inner().unwrap() {
        return Err(error.into());
    }
    drop(log);
    progress(report, ConversionPhase::Publishing, 0, None)?;
    legacy::remove_object_trees(data);
    Ok(())
}
