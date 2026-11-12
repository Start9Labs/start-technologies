use super::*;

#[test]
fn getter_callback_wire_parameters_contain_no_data() {
    let id = Arc::new(CallbackId(7));
    let handle = CallbackHandle(Arc::downgrade(&id));
    let mut registered = BTreeSet::from([id]);
    let params = handle.params(&mut registered).unwrap();
    assert_eq!(
        serde_json::to_value(params).unwrap(),
        serde_json::json!({"id": 7, "args": []})
    );
    assert!(registered.is_empty());
}

#[test]
fn cleared_getter_callback_does_not_send_a_notification() {
    let id = Arc::new(CallbackId(7));
    let handle = CallbackHandle(Arc::downgrade(&id));
    let mut registered = BTreeSet::from([id]);
    registered.clear();
    assert!(handle.params(&mut registered).is_none());
}

#[test]
fn replaced_callback_identity_cannot_consume_the_new_registration() {
    let old = Arc::new(CallbackId(7));
    let handle = CallbackHandle(Arc::downgrade(&old));
    let replacement = Arc::new(CallbackId(7));
    let mut registered = BTreeSet::from([replacement.clone()]);
    assert!(handle.params(&mut registered).is_none());
    assert!(Arc::ptr_eq(registered.first().unwrap(), &replacement));
}
