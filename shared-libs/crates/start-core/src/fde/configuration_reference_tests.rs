use super::*;

#[test]
fn previous_capture_requests_keep_the_native_generation_store_default() {
    let request = serde_json::json!({
        "baseRoot": "/base", "dataRoot": "/data", "networkProfiles": "/profiles",
        "source": {"kind": "legacy", "path": "/working"},
        "frozenFilesystems": [], "readonlyFilesystems": []
    });
    let mut epoch: ReadEpoch = serde_json::from_value(request).unwrap();
    assert_eq!(
        epoch.generation_store,
        crate::fde::boot::generation_store(Path::new(CONFIGURATION).parent().unwrap())
    );
    epoch.generation_store = "/explicit/store".into();
    let roundtrip: ReadEpoch =
        serde_json::from_value(serde_json::to_value(&epoch).unwrap()).unwrap();
    assert_eq!(roundtrip.generation_store, Path::new("/explicit/store"));
}
