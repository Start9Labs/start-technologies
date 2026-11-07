use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use rpc_toolkit::BoxWebSocket;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as NativeMessage;

/// Adapts a native socket to the shared message transport without adding a lifecycle.
pub fn transport<S>(socket: WebSocketStream<S>) -> BoxWebSocket
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    Box::new(
        socket
            .with(|message| {
                futures::future::ready(Ok::<_, tokio_tungstenite::tungstenite::Error>(to_native(
                    message,
                )))
            })
            .sink_map_err(axum::Error::new)
            .map(|result| result.map(from_native).map_err(axum::Error::new)),
    )
}

fn to_native(message: Message) -> NativeMessage {
    match message {
        Message::Text(text) => NativeMessage::Text(text.to_string().into()),
        Message::Binary(bytes) => NativeMessage::Binary(bytes),
        Message::Ping(bytes) => NativeMessage::Ping(bytes),
        Message::Pong(bytes) => NativeMessage::Pong(bytes),
        Message::Close(frame) => NativeMessage::Close(frame.map(|frame| {
            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }
        })),
    }
}

fn from_native(message: NativeMessage) -> Message {
    match message {
        NativeMessage::Text(text) => Message::Text(text.to_string().into()),
        NativeMessage::Binary(bytes) => Message::Binary(bytes),
        NativeMessage::Ping(bytes) => Message::Ping(bytes),
        NativeMessage::Pong(bytes) => Message::Pong(bytes),
        NativeMessage::Close(frame) => {
            Message::Close(frame.map(|frame| axum::extract::ws::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }))
        }
        NativeMessage::Frame(_) => unreachable!("WebSocketStream yields decoded messages"),
    }
}
