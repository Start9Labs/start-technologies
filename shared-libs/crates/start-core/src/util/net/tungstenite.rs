use axum::extract::ws::Message;
use futures::{Sink, SinkExt, Stream, StreamExt};
use rpc_toolkit::BoxWebSocket;
use tokio_tungstenite::tungstenite::Message as NativeMessage;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use crate::prelude::*;

#[cfg(test)]
mod tests;

/// Adapts a native socket to the shared message transport without adding a lifecycle.
pub fn transport<S, E>(socket: S) -> BoxWebSocket
where
    S: Stream<Item = Result<NativeMessage, E>>
        + Sink<NativeMessage, Error = E>
        + Unpin
        + Send
        + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    Box::new(
        socket
            .with(|message| futures::future::ready(Ok::<_, E>(to_native(message))))
            .sink_map_err(axum::Error::new)
            .map(|result| result.map(from_native).map_err(axum::Error::new)),
    )
}

/// Adapts the configured Reqwest WebSocket without another message mapping.
pub fn reqwest_transport(socket: reqwest_websocket::WebSocket) -> BoxWebSocket {
    transport(
        socket
            .sink_map_err(axum::Error::new)
            .with(|message: NativeMessage| {
                futures::future::ready(
                    reqwest_websocket::Message::try_from(message).map_err(axum::Error::new),
                )
            })
            .map(|message| message.map(NativeMessage::from).map_err(axum::Error::new)),
    )
}

/// Preserves the native decoded-message interface over a shared transport.
pub fn native_messages(
    socket: BoxWebSocket,
) -> impl Stream<Item = Result<NativeMessage, Error>> + Sink<NativeMessage, Error = Error> + Unpin + Send
{
    socket
        .sink_map_err(|error| Error::new(error, ErrorKind::Network))
        .with(|message| futures::future::ready(Ok::<_, Error>(from_native(message))))
        .map(|message| {
            message
                .map(to_native)
                .map_err(|error| Error::new(error, ErrorKind::Network))
        })
}

pub(crate) fn unexpected_eof() -> Error {
    Error::new(
        std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
        ErrorKind::Network,
    )
}

/// Requires an application close status; an empty close establishes no result.
pub fn close_result(frame: Option<CloseFrame>) -> Result<(), Error> {
    let frame = frame.ok_or_else(unexpected_eof)?;
    if frame.code == CloseCode::Normal {
        Ok(())
    } else {
        Err(Error::new(eyre!("{}", frame.reason), ErrorKind::Network))
    }
}

/// Ends on successful application Close, never on bare transport loss.
pub async fn next_text<S: Stream<Item = Result<NativeMessage, Error>> + Unpin>(
    socket: &mut S,
) -> Result<Option<tokio_tungstenite::tungstenite::Utf8Bytes>, Error> {
    loop {
        match socket.next().await.ok_or_else(unexpected_eof)?? {
            NativeMessage::Text(text) => return Ok(Some(text)),
            NativeMessage::Close(frame) => {
                close_result(frame)?;
                return Ok(None);
            }
            NativeMessage::Ping(_) | NativeMessage::Pong(_) => (),
            _ => {
                return Err(Error::new(
                    std::io::Error::from(std::io::ErrorKind::InvalidData),
                    ErrorKind::Network,
                ));
            }
        }
    }
}

/// Waits for the peer's application result after its data stream ends.
pub async fn receive_close_result<S: Stream<Item = Result<NativeMessage, Error>> + Unpin>(
    socket: &mut S,
) -> Result<(), Error> {
    while next_text(socket).await?.is_some() {}
    Ok(())
}

/// Reads an application JSON value; Close cannot substitute for that value.
pub async fn next_json<
    T: serde::de::DeserializeOwned,
    S: Stream<Item = Result<NativeMessage, Error>> + Unpin,
>(
    socket: &mut S,
) -> Result<T, Error> {
    let text = next_text(socket).await?.ok_or_else(unexpected_eof)?;
    serde_json::from_str(&text).with_kind(ErrorKind::Deserialization)
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
