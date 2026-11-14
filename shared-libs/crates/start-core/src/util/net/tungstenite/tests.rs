use futures::stream;

use super::*;

fn normal_close() -> NativeMessage {
    NativeMessage::Close(Some(CloseFrame {
        code: CloseCode::Normal,
        reason: "complete".into(),
    }))
}

#[tokio::test]
async fn text_stream_requires_a_result_and_preserves_captured_bytes() {
    let captured = "\u{feff}{\"integer\":18446744073709551615,\"float\":1.00e-20}";
    let mut socket = stream::iter([
        Ok(NativeMessage::Ping(vec![1].into())),
        Ok(NativeMessage::Text(captured.into())),
        Ok(normal_close()),
    ]);
    assert_eq!(
        next_text(&mut socket).await.unwrap().unwrap().as_str(),
        captured
    );
    assert!(next_text(&mut socket).await.unwrap().is_none());
}

#[tokio::test]
async fn physical_loss_and_empty_close_never_report_completion() {
    for messages in [vec![], vec![Ok(NativeMessage::Close(None))]] {
        let mut socket = stream::iter(messages);
        assert!(next_text(&mut socket).await.is_err());
    }
    let mut socket = stream::iter([Ok(NativeMessage::Close(Some(CloseFrame {
        code: CloseCode::Error,
        reason: "failed remotely".into(),
    })))]);
    let error = next_text(&mut socket).await.unwrap_err();
    assert!(error.to_string().contains("failed remotely"));
}

#[tokio::test]
async fn progress_termination_does_not_substitute_for_the_application_result() {
    let mut socket = stream::iter([
        Ok(NativeMessage::Text("null".into())),
        Err(unexpected_eof()),
    ]);
    assert_eq!(
        next_json::<Option<u64>, _>(&mut socket).await.unwrap(),
        None
    );
    assert!(receive_close_result(&mut socket).await.is_err());
}

#[tokio::test]
async fn verified_close_result_survives_later_transport_error() {
    let mut socket = stream::iter([Ok(normal_close()), Err(unexpected_eof())]);
    receive_close_result(&mut socket).await.unwrap();
}

#[tokio::test]
async fn progress_termination_preserves_remote_failure() {
    let mut socket = stream::iter([
        Ok(NativeMessage::Text("null".into())),
        Ok(NativeMessage::Close(Some(CloseFrame {
            code: CloseCode::Error,
            reason: "update failed".into(),
        }))),
    ]);
    assert_eq!(
        next_json::<Option<u64>, _>(&mut socket).await.unwrap(),
        None
    );
    assert!(
        receive_close_result(&mut socket)
            .await
            .unwrap_err()
            .to_string()
            .contains("update failed")
    );
}

#[tokio::test]
async fn close_cannot_replace_a_required_json_terminal() {
    let mut socket = stream::iter([Ok(normal_close())]);
    assert!(next_json::<Option<u64>, _>(&mut socket).await.is_err());
    let mut socket = stream::iter([
        Ok(NativeMessage::Text("18446744073709551615".into())),
        Ok(normal_close()),
    ]);
    assert_eq!(next_json::<u64, _>(&mut socket).await.unwrap(), u64::MAX);
    assert!(next_json::<Option<u64>, _>(&mut socket).await.is_err());
}
