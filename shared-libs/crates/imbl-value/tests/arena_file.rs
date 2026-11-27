#![cfg(feature = "arena")]

use imbl_value::{ord_json, OrdValue};

#[test]
fn file_backed_values_survive_writes_snapshots_and_drops() {
    let dir = tempfile::tempdir().unwrap();
    imbl_value::arena::init(dir.path()).unwrap();
    assert!(imbl_value::arena::init(dir.path()).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

    let mut value = ord_json!({ "series": [], "name": "arena" });
    assert!(imbl_value::arena::backing_len() > 0);
    let mut snapshots = Vec::new();
    for i in 0..5_000 {
        value["series"]
            .as_array_mut()
            .unwrap()
            .push_front(ord_json!({ "t": i, "label": format!("sample {i}") }));
        if i % 1_000 == 0 {
            snapshots.push(value.clone());
        }
    }
    assert_eq!(value["series"].as_array().unwrap().len(), 5_000);
    assert_eq!(value["series"][0]["label"], OrdValue::from("sample 4999"));
    assert_eq!(snapshots[1]["series"].as_array().unwrap().len(), 1_001);
    drop(snapshots);
    let json = serde_json::to_string(&value).unwrap();
    let back: OrdValue = serde_json::from_str(&json).unwrap();
    assert_eq!(back, value);
}
