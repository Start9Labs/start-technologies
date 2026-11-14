use super::*;

#[test]
fn swap_inventory_preserves_device_identity_and_excludes_the_header() {
    let inventory = "Filename\tType\tSize\tUsed\tPriority\n/dev/zram0 partition 1048576 0 5\n/swapfile file 2048 1024 -2\n";
    assert_eq!(
        swap_devices(inventory).collect::<Vec<_>>(),
        ["/dev/zram0", "/swapfile"]
    );
    assert!(
        swap_devices("Filename\tType\tSize\tUsed\tPriority\n")
            .next()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires root, system sbin PATH and exclusive ZRAM in an isolated managed VM; never run on the build host"]
async fn real_zram_lifecycle_reconciles_kernel_state() {
    let initially_active = zram_active().await.unwrap();
    disable_zram().await.unwrap();
    disable_zram().await.unwrap();
    assert!(!zram_active().await.unwrap());
    enable_zram().await.unwrap();
    enable_zram().await.unwrap();
    assert!(zram_active().await.unwrap());
    assert_ne!(
        crate::util::io::read_file_to_string("/sys/block/zram0/disksize")
            .await
            .unwrap()
            .trim(),
        "0"
    );
    let reader = std::fs::File::open(ZRAM_DEVICE).unwrap();
    let busy = disable_zram().await.unwrap_err();
    assert_eq!(
        busy.source
            .root_cause()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(libc::EBUSY)
    );
    drop(reader);
    disable_zram().await.unwrap();
    disable_zram().await.unwrap();
    assert!(!zram_active().await.unwrap());
    assert_eq!(
        crate::util::io::read_file_to_string("/sys/block/zram0/disksize")
            .await
            .unwrap()
            .trim(),
        "0"
    );
    if initially_active {
        enable_zram().await.unwrap();
    }
}
