use super::*;

#[tokio::test]
#[ignore = "requires a StartOS VM with LXC and root"]
async fn revocation_retains_controls_after_service_teardown_and_preserves_allowed_readers() {
    use crate::lxc::{LxcConfig, LxcManager};
    use crate::util::future::ControlledTask;
    let host_tasks = || {
        [
            ControlledTask::from(tokio::spawn(futures::future::pending::<()>())),
            ControlledTask::from(tokio::spawn(futures::future::pending::<()>())),
        ]
    };
    let first_tasks = host_tasks();
    let second_tasks = host_tasks();
    let manager = Arc::new(LxcManager::new());
    let first = manager.create(None, LxcConfig::default()).await.unwrap();
    let second = manager.create(None, LxcConfig::default()).await.unwrap();
    let volume = SharedVolumeId::new();
    let other = SharedVolumeId::new();
    let mut writer = Exposure {
        service: Weak::new(),
        runtime: ServiceRuntime::new(
            first.control().clone(),
            first_tasks[0].control(),
            first_tasks[1].control(),
        ),
        mounts: Arc::new(Mutex::new(SharedVolumeMounts::default())),
        volumes: BTreeMap::new(),
    };
    writer.admit(&volume, true);
    writer.admit(&volume, false);
    writer.admit(&volume, true);
    let mut reader = Exposure {
        service: Weak::new(),
        runtime: ServiceRuntime::new(
            second.control().clone(),
            second_tasks[0].control(),
            second_tasks[1].control(),
        ),
        mounts: Arc::new(Mutex::new(SharedVolumeMounts::default())),
        volumes: BTreeMap::new(),
    };
    reader.admit(&volume, true);
    let mut exposures = ServiceExposures {
        package_id: "consumer".parse().unwrap(),
        incarnations: BTreeMap::from([
            ((*first.guid).clone(), writer),
            ((*second.guid).clone(), reader),
        ]),
    };
    assert!(exposures.capture(&other, None).await.is_empty());
    assert!(
        exposures
            .capture(&volume, Some(&SharedVolumeGrant { readonly: false }))
            .await
            .is_empty()
    );
    let restrictive = exposures
        .capture(&volume, Some(&SharedVolumeGrant { readonly: true }))
        .await;
    assert_eq!(restrictive.len(), 1);
    restrictive[0].complete().await.unwrap();
    assert!(first_tasks.iter().all(|task| task.control().is_finished()));
    assert!(
        second_tasks
            .iter()
            .all(|task| !task.control().is_finished())
    );
    assert!(first.control().is_stopped().unwrap());
    assert!(!second.control().is_stopped().unwrap());
    assert!(
        exposures
            .capture(&volume, Some(&SharedVolumeGrant { readonly: true }))
            .await
            .is_empty()
    );
    let removed = exposures.capture(&volume, None).await;
    assert_eq!(removed.len(), 1);
    removed[0].complete().await.unwrap();
    assert!(second_tasks.iter().all(|task| task.control().is_finished()));
    assert!(second.control().is_stopped().unwrap());
    assert!(exposures.capture(&volume, None).await.is_empty());
    first.exit().await.unwrap();
    second.exit().await.unwrap();
}
