use std::path::Path;

use crate::error::BkfsError;
use crate::superblock::Superblock;
use crate::{BackupFS, BackupFSOptions, ConversionPhase, FormatStatus};

fn options(data: &Path) -> BackupFSOptions {
    BackupFSOptions {
        data_dir: data.to_owned(),
        password: "password".into(),
        readonly: false,
        file_size_padding: None,
        setuid_support: false,
        idmapped: false,
    }
}

#[test]
fn discovery_is_read_only_and_wrong_password_does_not_publish_a_pending_hint() {
    let data = tempfile::tempdir().unwrap();
    assert_eq!(super::inspect(data.path()).unwrap(), FormatStatus::Unknown);
    crate::legacy::fixture(data.path());
    let primary = data.path().join("superblock");
    let before = std::fs::read(&primary).unwrap();
    assert_eq!(super::inspect(data.path()).unwrap(), FormatStatus::Legacy);
    let mut config = options(data.path());
    config.password = "wrong".into();
    let mut events = Vec::new();
    assert!(BackupFS::convert_with_progress(config, |event| {
        events.push(event);
        Ok(())
    })
    .is_err());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, ConversionPhase::Inspecting);
    assert_eq!(super::inspect(data.path()).unwrap(), FormatStatus::Legacy);
    assert!(!data.path().join(super::PENDING_HINT).exists());
    assert_eq!(std::fs::read(primary).unwrap(), before);
}

#[test]
fn conversion_progress_stops_on_interruption_and_completes_after_durable_resumption() {
    use crate::ctrl::Controller;
    use crate::inode::{Inode, InodeAttributes};

    let data = tempfile::tempdir().unwrap();
    crate::legacy::fixture(data.path());
    {
        let ctrl = Controller::new(options(data.path())).unwrap();
        let mut file: InodeAttributes = ctrl.load(Inode(4)).unwrap();
        file.attrs.size = 1024 * crate::blockstore::CHUNK_SIZE;
        ctrl.save(&file).unwrap();
        ctrl.syncfs().unwrap();
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let path = data.path().to_owned();
    std::thread::spawn(move || {
        let mut first = Vec::new();
        let result = BackupFS::convert_with_progress(options(&path), |event| {
            first.push(event);
            if event.phase == ConversionPhase::Objects && event.done == 1 {
                return Err(BkfsError::unsupported("injected observer interruption"));
            }
            Ok(())
        });
        let _ = sender.send((result.is_err(), first));
    });
    let (failed, first) = receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("an interrupted conversion returns");
    assert!(failed);
    assert!(first
        .iter()
        .any(|event| event.phase == ConversionPhase::Records));
    assert!(!first
        .iter()
        .any(|event| event.phase == ConversionPhase::Complete));
    assert_eq!(
        super::inspect(data.path()).unwrap(),
        FormatStatus::Interrupted
    );
    assert!(BackupFS::new(options(data.path())).is_err());
    let mut resumed = Vec::new();
    BackupFS::convert_with_progress(options(data.path()), |event| {
        if event.phase == ConversionPhase::Complete {
            let sb = Superblock::load(&data.path().join("superblock"), "password", true)?;
            assert!(!sb.migration_pending);
            assert_eq!(sb.format_version, crate::superblock::FORMAT_VERSION);
            assert!(!data.path().join(super::PENDING_HINT).exists());
        }
        if let Some(previous) = resumed.last() {
            let previous: &crate::ConversionProgress = previous;
            if previous.phase == event.phase {
                assert!(event.done >= previous.done);
            }
        }
        resumed.push(event);
        Ok(())
    })
    .unwrap();
    let phases: Vec<_> = resumed.iter().map(|event| event.phase).collect();
    assert!(!phases.contains(&ConversionPhase::Records));
    for phase in [
        ConversionPhase::ScanningObjects,
        ConversionPhase::Objects,
        ConversionPhase::Publishing,
        ConversionPhase::Complete,
    ] {
        assert!(phases.contains(&phase));
    }
    let objects = resumed
        .iter()
        .rfind(|event| event.phase == ConversionPhase::Objects)
        .unwrap();
    assert_eq!(objects.total, Some(objects.done));
    assert_eq!(resumed.last().unwrap().phase, ConversionPhase::Complete);
    assert_eq!(super::inspect(data.path()).unwrap(), FormatStatus::Current);
    let fs = BackupFS::new(options(data.path())).unwrap();
    assert!(!fs.handler.lock().unwrap().ctrl().is_legacy());
}

#[test]
fn a_stale_discovery_hint_does_not_authorize_migration_or_block_a_current_store() {
    let data = tempfile::tempdir().unwrap();
    drop(BackupFS::new(options(data.path())).unwrap());
    std::fs::write(data.path().join(super::PENDING_HINT), b"not authoritative").unwrap();
    assert_eq!(
        super::inspect(data.path()).unwrap(),
        FormatStatus::Interrupted
    );
    drop(BackupFS::new(options(data.path())).unwrap());
    let anchor = data.path().join("segments/state");
    let before = std::fs::read(&anchor).unwrap();
    BackupFS::convert(options(data.path())).unwrap();
    assert_eq!(super::inspect(data.path()).unwrap(), FormatStatus::Current);
    assert_eq!(std::fs::read(anchor).unwrap(), before);
}
