use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, Utf8Bytes};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::{Sink, SinkExt, Stream, StreamExt};
use rpc_toolkit::BoxWebSocket;
use snow::{HandshakeState, TransportState};
use tokio_util::codec::length_delimited::Builder;
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

use super::{MAX_CLOSE_PAYLOAD_BYTES, MAX_CLOSE_REASON_BYTES};
use crate::prelude::*;
use crate::sign::AnyVerifyingKey;
use crate::sign::noise::{NoiseIdentity, authenticate_peer};

/// Logical RPC response limit before framing and encryption overhead.
pub const DEFAULT_RPC_RESPONSE_LIMIT: usize = 64 * 1024 * 1024;

const RECORD_HEADER: usize = 2;
const MESSAGE_HEADER: usize = 4;
const MESSAGE_KIND: usize = 1;
const TAG_SIZE: usize = 16;
const MAX_RECORD: usize = u16::MAX as usize;
const MAX_PLAINTEXT: usize = MAX_RECORD - TAG_SIZE;
const MAX_HANDSHAKE_RECORD: usize = 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
#[derive(num_enum::IntoPrimitive, num_enum::TryFromPrimitive)]
#[repr(u8)]
enum MessageKind {
    Text = 0,
    Binary = 1,
    Close = 2,
}

pub(crate) fn browser_profile() -> serde_json::Value {
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    let logical = NonZeroUsize::new(DEFAULT_RPC_RESPONSE_LIMIT).unwrap();
    let mut close_codes: Vec<[u16; 2]> = Vec::new();
    for code in 0..=u16::MAX {
        if !CloseCode::from(code).is_allowed() {
            continue;
        }
        match close_codes.last_mut() {
            Some([_, end]) if end.checked_add(1) == Some(code) => *end = code,
            _ => close_codes.push([code, code]),
        }
    }
    serde_json::json!({
        "recordHeaderBytes": RECORD_HEADER,
        "messageHeaderBytes": MESSAGE_HEADER,
        "messageKindBytes": MESSAGE_KIND,
        "maxRecordBytes": MAX_RECORD,
        "maxPlaintextBytes": MAX_PLAINTEXT,
        "maxHandshakeRecordBytes": MAX_HANDSHAKE_RECORD,
        "handshakeTimeoutMs": HANDSHAKE_TIMEOUT.as_millis(),
        "writeTimeoutMs": Duration::from_secs(15).as_millis(),
        "maxIncomingBytes": logical.get(),
        "maxWireBytes": wire_message_size(logical).get(),
        "maxWireChunkBytes": wire_chunk_size(logical).get(),
        "closeStatusBytes": std::mem::size_of::<u16>(),
        "normalCloseCode": u16::from(CloseCode::Normal),
        "noStatusCloseCode": u16::from(CloseCode::Status),
        "abnormalCloseCode": u16::from(CloseCode::Abnormal),
        "maxClosePayloadBytes": MAX_CLOSE_PAYLOAD_BYTES,
        "maxCloseReasonBytes": MAX_CLOSE_REASON_BYTES,
        "closeCodes": close_codes,
        "kinds": {
            "text": u8::from(MessageKind::Text),
            "binary": u8::from(MessageKind::Binary),
            "close": u8::from(MessageKind::Close),
        },
    })
}

fn validate_close(frame: &CloseFrame) -> Result<(), Error> {
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    if frame.reason.len() > MAX_CLOSE_REASON_BYTES || !CloseCode::from(frame.code).is_allowed() {
        return Err(invalid_message());
    }
    Ok(())
}

fn decode_close(mut payload: Bytes) -> Result<Option<CloseFrame>, Error> {
    if payload.is_empty() {
        return Ok(None);
    }
    if payload.len() < std::mem::size_of::<u16>() {
        return Err(invalid_message());
    }
    let frame = CloseFrame {
        code: payload.get_u16(),
        reason: Utf8Bytes::try_from(payload).map_err(|_| invalid_message())?,
    };
    validate_close(&frame)?;
    Ok(Some(frame))
}

fn framing(header: usize, maximum: usize) -> Builder {
    let mut builder = LengthDelimitedCodec::builder();
    builder
        .big_endian()
        .length_field_length(header)
        .max_frame_length(maximum);
    builder
}

fn retaining_decoder(header: usize, maximum: usize) -> LengthDelimitedCodec {
    framing(header, maximum)
        .length_adjustment(header as isize)
        .num_skip(0)
        .new_codec()
}

fn invalid_message() -> Error {
    Error::new(
        eyre!("{}", t!("util.net.secure.invalid-message")),
        ErrorKind::InvalidRequest,
    )
}

/// Inbound wire budget covering the handshake and one framed logical data message.
pub fn wire_message_size(logical: NonZeroUsize) -> NonZeroUsize {
    let plain = logical.get().saturating_add(MESSAGE_HEADER + MESSAGE_KIND);
    let records = plain.div_ceil(MAX_PLAINTEXT);
    NonZeroUsize::new(
        plain
            .saturating_add(records.saturating_mul(RECORD_HEADER + TAG_SIZE))
            .max(MAX_HANDSHAKE_RECORD + RECORD_HEADER),
    )
    .unwrap()
}

/// Raw chunk allowance for a bounded logical payload, including handshake and record framing.
pub fn wire_chunk_size(logical: NonZeroUsize) -> NonZeroUsize {
    wire_message_size(logical).min(NonZeroUsize::new(MAX_RECORD + RECORD_HEADER).unwrap())
}

struct NoiseCodec {
    state: TransportState,
    incoming: NonZeroUsize,
    record_reader: LengthDelimitedCodec,
    record_writer: LengthDelimitedCodec,
    records: BytesMut,
    message_reader: LengthDelimitedCodec,
    message_writer: LengthDelimitedCodec,
    messages: BytesMut,
    plaintext: Vec<u8>,
}

impl NoiseCodec {
    fn new(state: TransportState, incoming: NonZeroUsize, records: BytesMut) -> Self {
        Self {
            state,
            incoming,
            record_reader: retaining_decoder(RECORD_HEADER, MAX_RECORD),
            record_writer: framing(RECORD_HEADER, MAX_RECORD).new_codec(),
            records,
            message_reader: retaining_decoder(
                MESSAGE_HEADER,
                incoming
                    .get()
                    .max(MAX_CLOSE_PAYLOAD_BYTES)
                    .saturating_add(MESSAGE_KIND),
            ),
            message_writer: framing(MESSAGE_HEADER, u32::MAX as usize).new_codec(),
            messages: BytesMut::new(),
            plaintext: vec![0; MAX_RECORD],
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.records.extend_from_slice(bytes);
    }

    fn has_partial_message(&self) -> bool {
        !self.records.is_empty() || !self.messages.is_empty()
    }

    fn next_message(&mut self) -> Result<Option<Message>, Error> {
        loop {
            if let Some(mut message) = self
                .message_reader
                .decode(&mut self.messages)
                .map_err(|_| invalid_message())?
            {
                message.advance(MESSAGE_HEADER);
                if message.is_empty() {
                    return Err(invalid_message());
                }
                let kind =
                    MessageKind::try_from(message.get_u8()).map_err(|_| invalid_message())?;
                if !matches!(kind, MessageKind::Close) && message.len() > self.incoming.get() {
                    return Err(invalid_message());
                }
                return Ok(Some(match kind {
                    MessageKind::Text => Message::Text(
                        Utf8Bytes::try_from(message.freeze()).map_err(|_| invalid_message())?,
                    ),
                    MessageKind::Binary => Message::Binary(message.freeze()),
                    MessageKind::Close => Message::Close(decode_close(message.freeze())?),
                }));
            }
            let Some(mut record) = self
                .record_reader
                .decode(&mut self.records)
                .map_err(|_| invalid_message())?
            else {
                return Ok(None);
            };
            record.advance(RECORD_HEADER);
            let length = self
                .state
                .read_message(&record, &mut self.plaintext)
                .with_kind(ErrorKind::InvalidSignature)?;
            self.messages.extend_from_slice(&self.plaintext[..length]);
        }
    }

    fn seal(&mut self, message: Message) -> Result<VecDeque<Message>, Error> {
        let (kind, data): (MessageKind, Bytes) = match message {
            Message::Text(text) => (MessageKind::Text, text.into()),
            Message::Binary(data) => (MessageKind::Binary, data),
            Message::Close(frame) => {
                let mut payload = BytesMut::new();
                if let Some(frame) = frame {
                    validate_close(&frame)?;
                    payload.put_u16(frame.code);
                    payload.extend_from_slice(frame.reason.as_bytes());
                }
                (MessageKind::Close, payload.freeze())
            }
            control => return Ok(VecDeque::from([control])),
        };
        let closing = matches!(kind, MessageKind::Close);
        let mut body = BytesMut::with_capacity(MESSAGE_KIND + data.len());
        body.extend_from_slice(&[kind.into()]);
        body.extend_from_slice(&data);
        let mut framed = BytesMut::new();
        self.message_writer
            .encode(body.freeze(), &mut framed)
            .map_err(|_| invalid_message())?;
        let mut output = VecDeque::new();
        let mut encrypted = vec![0; MAX_RECORD];
        for plaintext in framed.chunks(MAX_PLAINTEXT) {
            let length = self
                .state
                .write_message(plaintext, &mut encrypted)
                .with_kind(ErrorKind::InvalidSignature)?;
            let mut record = BytesMut::new();
            self.record_writer
                .encode(Bytes::copy_from_slice(&encrypted[..length]), &mut record)
                .map_err(|_| invalid_message())?;
            output.push_back(Message::Binary(record.freeze()));
        }
        if closing {
            output.push_back(Message::Close(None));
        }
        Ok(output)
    }
}

struct HandshakeRecords {
    decoder: LengthDelimitedCodec,
    buffered: BytesMut,
}

impl HandshakeRecords {
    fn new() -> Self {
        Self {
            decoder: retaining_decoder(RECORD_HEADER, MAX_HANDSHAKE_RECORD),
            buffered: BytesMut::new(),
        }
    }

    async fn receive(
        &mut self,
        socket: &mut BoxWebSocket,
        state: &mut HandshakeState,
    ) -> Result<Vec<u8>, Error> {
        loop {
            if let Some(mut record) = self
                .decoder
                .decode(&mut self.buffered)
                .map_err(|_| invalid_message())?
            {
                record.advance(RECORD_HEADER);
                let mut payload = vec![0; MAX_HANDSHAKE_RECORD];
                let length = state
                    .read_message(&record, &mut payload)
                    .with_kind(ErrorKind::InvalidSignature)?;
                payload.truncate(length);
                return Ok(payload);
            }
            match socket.next().await {
                Some(Ok(Message::Binary(bytes))) => self.buffered.extend_from_slice(&bytes),
                Some(Ok(Message::Ping(_))) => socket.flush().await.with_kind(ErrorKind::Network)?,
                Some(Ok(Message::Pong(_))) => (),
                Some(Err(error)) => return Err(Error::new(error, ErrorKind::Network)),
                Some(Ok(Message::Close(_))) | None => {
                    return Err(Error::new(
                        std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
                        ErrorKind::Network,
                    ));
                }
                _ => return Err(invalid_message()),
            }
        }
    }
}

async fn send_handshake(
    socket: &mut BoxWebSocket,
    state: &mut HandshakeState,
    payload: &[u8],
) -> Result<(), Error> {
    let mut output = vec![0; MAX_HANDSHAKE_RECORD];
    let length = state
        .write_message(payload, &mut output)
        .with_kind(ErrorKind::InvalidSignature)?;
    let mut record = BytesMut::new();
    framing(RECORD_HEADER, MAX_HANDSHAKE_RECORD)
        .new_codec()
        .encode(Bytes::copy_from_slice(&output[..length]), &mut record)
        .map_err(|_| invalid_message())?;
    socket
        .send(Message::Binary(record.freeze()))
        .await
        .with_kind(ErrorKind::Network)
}

/// An authenticated Noise channel retaining WebSocket text, binary and control semantics.
/// Close results are encrypted; physical controls carry no application close reason.
/// Peer authentication does not enroll keys or grant RPC roles.
pub struct EncryptedSocket {
    inner: BoxWebSocket,
    codec: NoiseCodec,
    pending: VecDeque<Message>,
    closing: Option<Message>,
    fused: bool,
}

impl EncryptedSocket {
    fn new(
        inner: BoxWebSocket,
        state: HandshakeState,
        records: HandshakeRecords,
        incoming: NonZeroUsize,
    ) -> Result<Self, Error> {
        Ok(Self {
            inner,
            codec: NoiseCodec::new(
                state
                    .into_transport_mode()
                    .with_kind(ErrorKind::Authorization)?,
                incoming,
                records.buffered,
            ),
            pending: VecDeque::new(),
            closing: None,
            fused: false,
        })
    }

    /// Completes the initiator exchange before returning the verified remote identity.
    /// Preparing the identity separately keeps interactive signing outside the handshake deadline.
    pub async fn initiate(
        mut socket: BoxWebSocket,
        identity: NoiseIdentity,
        prologue: &[u8],
        incoming: NonZeroUsize,
    ) -> Result<(AnyVerifyingKey, Self), Error> {
        tokio::time::timeout(HANDSHAKE_TIMEOUT, async move {
            let mut state = identity.initiator(prologue)?;
            let mut records = HandshakeRecords::new();
            send_handshake(&mut socket, &mut state, &[]).await?;
            let certificate = records.receive(&mut socket, &mut state).await?;
            let peer = authenticate_peer(&state, &certificate)?;
            send_handshake(&mut socket, &mut state, identity.certificate()).await?;
            Ok((peer, Self::new(socket, state, records, incoming)?))
        })
        .await
        .with_kind(ErrorKind::Network)?
    }

    /// Completes the responder exchange, optionally restricting the admitted peer identity.
    pub async fn accept(
        mut socket: BoxWebSocket,
        identity: NoiseIdentity,
        prologue: &[u8],
        incoming: NonZeroUsize,
        expected: Option<AnyVerifyingKey>,
    ) -> Result<(AnyVerifyingKey, Self), Error> {
        tokio::time::timeout(HANDSHAKE_TIMEOUT, async move {
            let mut state = identity.responder(prologue)?;
            let mut records = HandshakeRecords::new();
            if !records.receive(&mut socket, &mut state).await?.is_empty() {
                return Err(invalid_message());
            }
            send_handshake(&mut socket, &mut state, identity.certificate()).await?;
            let certificate = records.receive(&mut socket, &mut state).await?;
            let peer = authenticate_peer(&state, &certificate)?;
            if expected.as_ref().is_some_and(|expected| expected != &peer) {
                return Err(Error::new(
                    eyre!("{}", t!("util.net.secure.unexpected-peer")),
                    ErrorKind::Authorization,
                ));
            }
            Ok((peer, Self::new(socket, state, records, incoming)?))
        })
        .await
        .with_kind(ErrorKind::Network)?
    }

    fn fail(&mut self, error: Error) -> axum::Error {
        self.fused = true;
        self.pending.clear();
        axum::Error::new(error.source)
    }

    fn poll_pending(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), axum::Error>> {
        while !self.pending.is_empty() {
            ready!(self.inner.poll_ready_unpin(cx))?;
            self.inner
                .start_send_unpin(self.pending.pop_front().unwrap())?;
        }
        Poll::Ready(Ok(()))
    }
}

impl Stream for EncryptedSocket {
    type Item = Result<Message, axum::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.fused {
            return Poll::Ready(None);
        }
        loop {
            if this.closing.is_some() {
                // Authenticated close results survive failed transport acknowledgements.
                if ready!(this.poll_pending(cx)).is_ok() {
                    let _ = ready!(this.inner.poll_flush_unpin(cx));
                }
                this.fused = true;
                return Poll::Ready(Some(Ok(this.closing.take().unwrap())));
            }
            match this.codec.next_message() {
                Ok(Some(message @ Message::Close(_))) => {
                    if this.codec.has_partial_message() {
                        return Poll::Ready(Some(Err(this.fail(invalid_message()))));
                    }
                    this.closing = Some(message);
                    this.pending.push_back(Message::Close(None));
                    continue;
                }
                Ok(Some(message)) => return Poll::Ready(Some(Ok(message))),
                Ok(None) => (),
                Err(error) => return Poll::Ready(Some(Err(this.fail(error)))),
            }
            match ready!(this.inner.poll_next_unpin(cx)) {
                Some(Ok(Message::Binary(bytes))) => this.codec.feed(&bytes),
                Some(Ok(Message::Text(_))) => {
                    return Poll::Ready(Some(Err(this.fail(invalid_message()))));
                }
                Some(Ok(Message::Close(_))) | None if this.codec.has_partial_message() => {
                    return Poll::Ready(Some(Err(this.fail(invalid_message()))));
                }
                Some(Ok(Message::Close(_))) => {
                    this.fused = true;
                    return Poll::Ready(Some(Ok(Message::Close(None))));
                }
                other => return Poll::Ready(other),
            }
        }
    }
}

impl Sink<Message> for EncryptedSocket {
    type Error = axum::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        if this.fused {
            return Poll::Ready(Err(axum::Error::new(std::io::Error::from(
                std::io::ErrorKind::BrokenPipe,
            ))));
        }
        ready!(this.poll_pending(cx))?;
        this.inner.poll_ready_unpin(cx)
    }

    fn start_send(self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        let this = self.get_mut();
        this.pending = this.codec.seal(message).map_err(|error| this.fail(error))?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        ready!(this.poll_pending(cx))?;
        this.inner.poll_flush_unpin(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        ready!(this.poll_pending(cx))?;
        this.inner.poll_close_unpin(cx)
    }
}

#[cfg(test)]
pub(crate) mod tests;
