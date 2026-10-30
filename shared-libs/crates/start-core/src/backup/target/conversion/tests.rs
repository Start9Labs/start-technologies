use super::*;

fn target(id: u32) -> BackupTargetId {
    BackupTargetId::Cifs { id }
}

#[test]
fn conversion_reservations_exclude_the_same_backup_but_not_other_backups() {
    let mut jobs = ConversionJobs::new();
    reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).unwrap();
    assert!(reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).is_err());
    reserve(&mut jobs, target(1), "server-b".into(), FullProgress::new()).unwrap();
    reserve(&mut jobs, target(2), "server-a".into(), FullProgress::new()).unwrap();
    assert_eq!(jobs["cifs-1"].len(), 2);
    assert_eq!(jobs["cifs-2"].len(), 1);
}

#[test]
fn failed_and_completed_attempts_allow_an_explicit_retry() {
    for result in [
        Ok(()),
        Err(Error::new(eyre!("injected failure"), ErrorKind::Backup)),
    ] {
        let mut jobs = ConversionJobs::new();
        reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).unwrap();
        let status = ConversionJob::finished(FullProgress::new(), &result);
        assert_eq!(status.error.is_some(), result.is_err());
        assert_eq!(status.state == ConversionState::Failed, result.is_err());
        jobs.get_mut("cifs-1")
            .unwrap()
            .insert("server-a".into(), status);
        reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).unwrap();
        assert_eq!(jobs["cifs-1"]["server-a"].state, ConversionState::Running);
        assert!(jobs["cifs-1"]["server-a"].error.is_none());
    }
}
