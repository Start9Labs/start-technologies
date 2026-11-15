use serde_json::json;

use super::*;

#[test]
fn stream_events_round_trip_full_width_and_string_ids() {
    for id in [json!(u64::MAX), json!("watch")] {
        let wire = json!({"rpcstream":"1.0", "id":id, "result":{"patch":[]}, "end":false});
        let response: RpcStreamResponse = serde_json::from_value(wire.clone()).unwrap();
        assert!(!response.end);
        assert!(response.result.is_ok());
        assert_eq!(serde_json::to_value(&response).unwrap(), wire);
        assert_eq!(serde_json::to_value(response.into_value()).unwrap(), wire);
    }
}

#[test]
fn terminal_values_and_errors_use_the_same_response_envelope() {
    for wire in [
        json!({"rpcstream":"1.0", "id":42, "result":null, "end":true}),
        json!({"rpcstream":"1.0", "id":42, "result":{"count":5}, "end":true}),
        json!({"rpcstream":"1.0", "id":42, "error":{"code":-32800,"message":"Response stream canceled"}, "end":true}),
    ] {
        let response: RpcStreamResponse = serde_json::from_value(wire.clone()).unwrap();
        assert!(response.end);
        assert_eq!(response.result.is_err(), wire.get("error").is_some());
        assert_eq!(serde_json::to_value(response).unwrap(), wire);
    }
}

#[test]
fn malformed_stream_envelopes_are_rejected() {
    for wire in [
        json!({"rpcstream":"2.0", "id":42, "result":null, "end":false}),
        json!({"rpcstream":1, "id":42, "result":null, "end":false}),
        json!({"id":42, "result":null, "end":false}),
        json!({"rpcstream":"1.0", "id":42, "result":null}),
        json!({"rpcstream":"1.0", "id":42, "result":null, "end":"false"}),
        json!({"rpcstream":"1.0", "id":null, "result":null, "end":false}),
        json!({"rpcstream":"1.0", "result":null, "end":false}),
        json!({"rpcstream":"1.0", "id":42, "end":false}),
        json!({"rpcstream":"1.0", "id":42, "result":null, "error":{"code":-1,"message":"ambiguous"}, "end":true}),
        json!({"rpcstream":"1.0", "jsonrpc":"2.0", "id":42, "result":null, "end":false}),
    ] {
        assert!(
            serde_json::from_value::<RpcStreamResponse>(wire.clone()).is_err(),
            "{}",
            wire
        );
    }
}
