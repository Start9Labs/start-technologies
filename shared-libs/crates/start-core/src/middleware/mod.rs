pub mod auth;
pub mod connect_info;
pub mod cors;
pub mod db;
pub mod json;

pub(crate) fn params_object(
    params: &mut imbl_value::Value,
) -> Result<
    &mut imbl_value::InOMap<imbl_value::InternedString, imbl_value::Value>,
    rpc_toolkit::yajrc::RpcError,
> {
    if params.is_null() {
        *params = imbl_value::Value::Object(Default::default());
    }
    params
        .as_object_mut()
        .ok_or(rpc_toolkit::yajrc::INVALID_PARAMS_ERROR)
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    #[test]
    fn parameter_objects_preserve_fields_and_materialize_null() {
        for (mut params, expected) in [
            (json!(null), json!({"injected": true})),
            (
                json!({"existing": "retained"}),
                json!({"existing": "retained", "injected": true}),
            ),
        ] {
            params_object(&mut params)
                .unwrap()
                .insert("injected".into(), json!(true));
            assert_eq!(params, expected);
        }
    }

    #[test]
    fn parameter_injection_rejects_non_objects_without_mutation() {
        for mut params in [
            json!([]),
            json!([{}]),
            json!(true),
            json!(7),
            json!("value"),
        ] {
            let original = params.clone();
            let error = params_object(&mut params).unwrap_err();
            assert_eq!(error.code, rpc_toolkit::yajrc::INVALID_PARAMS_ERROR.code);
            assert_eq!(params, original);
        }
    }
}
