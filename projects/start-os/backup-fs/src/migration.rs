use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use chacha20::Key;
use log::warn;

use crate::atomic_file::AtomicFile;
use crate::error::{BkfsErrorKind, BkfsResult};
use crate::legacy;
use crate::objects::{Inventory, ObjectVersion};
use crate::seglog::{self, Frames, Location, Record, Roots, SegmentWriter};
use crate::serde::{data_config, decode};
use crate::snapshot::{Catalog, Snapshot};
use crate::state::{Counters, State, View};
use crate::superblock::Constants;
use crate::vault::{self, AuthenticationTag, Binding, EccParams};

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
    let plaintext = if vault::is_legacy(blob) {
        vault::open_legacy(blob, key)?
    } else {
        vault::open(blob, key)?
    };
    Ok(UpgradePlain {
        plaintext,
        current_tag: None,
    })
}

fn upgraded_blob(blob: &[u8], key: &Key, binding: Binding) -> BkfsResult<Option<Vec<u8>>> {
    match open_for_upgrade(blob, key, binding) {
        Ok(opened) if opened.current_tag.is_some() => Ok(None),
        Ok(opened) => Ok(Some(
            vault::seal_bound(
                &opened.plaintext,
                key,
                EccParams {
                    data: blob[5],
                    parity: blob[6],
                },
                binding,
            )
            .bytes,
        )),
        Err(error) if matches!(error.kind, BkfsErrorKind::BadChecksum) => {
            warn!("leaving unreadable blob unmigrated");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn convert_roots(roots: &legacy::Roots, locations: &HashMap<(u64, u64), Location>) -> Roots {
    let mut converted = Roots::default();
    for (key, old) in roots.entries() {
        let location = locations
            .get(&(old.segment, old.offset))
            .copied()
            .unwrap_or(Location {
                segment: old.segment,
                offset: old.offset,
                len: old.len,
                ..Default::default()
            });
        converted.update(key, Some(location));
    }
    converted
}

fn migrate_object(
    source: &Path,
    destination: &Path,
    key: &Key,
    binding: Binding,
) -> BkfsResult<()> {
    if source != destination {
        match fs::read(destination) {
            Ok(bytes) if vault::open_bound(&bytes, key, binding).is_ok() => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let bytes = match fs::read(source) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let upgraded = upgraded_blob(&bytes, key, binding)?;
    if upgraded.is_none() && source == destination {
        return Ok(());
    }
    let mut file = AtomicFile::create_buffered(destination.to_owned())?;
    file.write_all(upgraded.as_deref().unwrap_or(&bytes))?;
    if source == destination {
        file.save_durable()
    } else {
        file.save_fast()
    }
}

fn retire_sources(backing: &File, sources: &mut Vec<PathBuf>) -> BkfsResult<()> {
    if sources.is_empty() {
        return Ok(());
    }
    crate::util::sync_filesystem(backing)?;
    for source in sources.drain(..) {
        fs::remove_file(source)?;
    }
    crate::util::sync_filesystem(backing)?;
    Ok(())
}

/// Requires the exclusive store lock and authenticated pending marker.
pub(crate) fn reseal_store(data: &Path, key: &Key, constants: Constants) -> BkfsResult<()> {
    let dir = data.join("segments");
    fs::create_dir_all(&dir)?;
    if dir.join("state").try_exists()? {
        State::open(&dir, key)?;
        return Ok(());
    }
    let mut source = legacy::Source::load(data, key)?;
    let mut locations = HashMap::new();
    let mut versions = HashSet::new();
    let mut stable = HashMap::new();
    for (segment, _) in seglog::list_segments(&dir)? {
        let path = seglog::segment_path(&dir, segment);
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
                            warn!("leaving undecodable log record unmigrated");
                            locations.insert((segment, frame.offset as u64), location);
                            continue;
                        }
                    };
                    let migrated = match record {
                        Record::Object {
                            id,
                            version: ObjectVersion::Version(token),
                        } => {
                            versions.insert((id, token));
                            (token == vault::migration_version(key, id)?).then_some(id)
                        }
                        _ => None,
                    };
                    // Migration pointers must not override the legacy source on retry.
                    if migrated.is_some() {
                        source.advance_sequence(frame.seq)?;
                    } else {
                        source.observe(
                            &record,
                            frame.seq,
                            legacy::Location {
                                segment,
                                offset: frame.offset as u64,
                                len: frame.len,
                            },
                        )?;
                    }
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
                            // Preserve legacy snapshot offsets, including recoverable parity truncation.
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
                    if let Some(id) = migrated {
                        stable.insert(id, location);
                    }
                }
                Err(error) if matches!(error.kind, BkfsErrorKind::BadChecksum) => {
                    warn!("leaving unreadable log record unmigrated")
                }
                Err(error) => return Err(error),
            }
            locations.insert((segment, frame.offset as u64), location);
        }
        if changed {
            let mut file = AtomicFile::create_buffered(path)?;
            file.write_all(&rewritten)?;
            file.save_durable()?;
        }
    }
    for (id, token) in versions {
        let path = id.path(data, Some(ObjectVersion::Version(token))).unwrap();
        migrate_object(&path, &path, key, Binding::Object { id, version: token })?;
    }
    let mut roots = convert_roots(&source.roots, &locations);
    let inventory = Inventory::load(data)?;
    let mut writer = None;
    let backing = File::open(data)?;
    let mut retired = Vec::new();
    for id in inventory.legacy_ids() {
        let token = vault::migration_version(key, id)?;
        let version = ObjectVersion::Version(token);
        let original = id.path(data, None).unwrap();
        let mut stage = || -> BkfsResult<()> {
            migrate_object(
                &original,
                &id.path(data, Some(version)).unwrap(),
                key,
                Binding::Object { id, version: token },
            )?;
            if !stable.contains_key(&id) {
                let record = seglog::seal_object(key, constants.ecc(), id, version)?;
                if writer.is_none() {
                    writer = Some(SegmentWriter::new(&dir, constants.segment_size)?);
                }
                let seq = source.next_seq;
                source.advance_sequence(seq)?;
                let location = writer.as_mut().unwrap().append(seq, &record)?;
                stable.insert(id, location);
            }
            Ok(())
        };
        if let Err(error) = stage() {
            if !matches!(&error.kind, BkfsErrorKind::Io(io) if io.raw_os_error() == Some(libc::ENOSPC))
                || retired.is_empty()
            {
                return Err(error);
            }
            retire_sources(&backing, &mut retired)?;
            stage()?;
        }
        retired.push(original);
        if retired.len() >= crate::util::sync_batch() {
            retire_sources(&backing, &mut retired)?;
        }
    }
    retire_sources(&backing, &mut retired)?;
    for references in source.storage.values() {
        references.visit_objects(key, |id| {
            if let Some(&location) = stable.get(&id) {
                roots.objects.entry(id).or_insert(location);
            }
        });
    }
    let counters = Counters {
        next_seq: source.next_seq,
        max_inode: source.max_inode,
    };
    let mut catalog = Catalog::default();
    for (name, old) in source.snapshots {
        let mut snapshot = convert_roots(&old.roots, &locations);
        for id in old.legacy {
            if let Some(&location) = stable.get(&id) {
                snapshot.objects.entry(id).or_insert(location);
            }
        }
        let (view, _) = View::write_checkpoint(&dir, key, constants.ecc(), &snapshot, counters)?;
        catalog
            .entries
            .insert(name, Snapshot::new(old.info, view, &snapshot));
    }
    let catalog = if catalog.entries.is_empty() {
        None
    } else {
        Some(catalog.write(&dir, key, constants.ecc())?)
    };
    State::create(&dir, key, constants.ecc(), &roots, counters, catalog)?;
    File::open(data)?.sync_all()?;
    Ok(())
}
