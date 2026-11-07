use super::*;

#[test]
fn cleared_progress_succeeds_only_after_staging_the_update() {
    assert_eq!(
        progress_outcome(Some(RestartReason::Update)).unwrap(),
        "complete"
    );
    assert!(progress_outcome(None).is_err());
}
