use futures::{SinkExt, StreamExt};
use patch_db::{Dump, Revision};
use rpc_toolkit::{RPCSTREAM_VERSION, RpcResponse, RpcStreamResponse};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::*;
use crate::middleware::auth::signature::rpc::RPC_SIGNATURE_FIELD;
use crate::middleware::auth::signature::{SignatureHeader, SigningContext};
use crate::sign::Signer;

const MAX_RESPONSE_SIZE: usize = 64 * 1024 * 1024;

pub(super) fn websocket_url(registry: &Url) -> Result<Url, Error> {
    let mut url = registry.clone();
    url.path_segments_mut()
        .map_err(|_| Error::new(eyre!("invalid registry URL"), ErrorKind::ParseUrl))?
        .pop_if_empty()
        .push("ws")
        .push("rpc");
    let scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        _ => {
            return Err(Error::new(
                eyre!("invalid registry scheme"),
                ErrorKind::ParseUrl,
            ));
        }
    };
    url.set_scheme(scheme)
        .map_err(|_| Error::new(eyre!("invalid registry scheme"), ErrorKind::ParseUrl))?;
    Ok(url)
}

pub(super) fn response_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(4096)
        .max_message_size(Some(MAX_RESPONSE_SIZE))
        .max_frame_size(Some(MAX_RESPONSE_SIZE))
}

pub(super) async fn subscription_request(
    subscription: &Subscription,
    os_version: &exver::Version,
    signer: &Signer,
) -> Result<Value, Error> {
    let mut request = imbl_value::json!({
        "jsonrpc": "2.0", "rpcstream": RPCSTREAM_VERSION, "id": subscription.request_id(),
        "method": "db.subscribe", "params": { "pointer": subscription.pointer(), "osVersion": os_version }
    });
    let proof = SignatureHeader::sign_rpc(
        signer,
        &request,
        subscription
            .registry()
            .host_str()
            .or_not_found("registry hostname")?,
    )
    .await?
    .to_header();
    request[RPC_SIGNATURE_FIELD] = Value::from(proof.to_str().with_kind(ErrorKind::Serialization)?);
    Ok(request)
}

pub(super) async fn subscribe(
    db: &TypedPatchDb<Database>,
    subscriptions: &BTreeSet<Subscription>,
    context: &ContextProvider,
    backoff: &mut u64,
) -> Result<(), Error> {
    let (socket, device, signer) = {
        let Some(ctx) = context() else { return Ok(()) };
        let registry = subscriptions.first().unwrap().registry();
        let device = DeviceInfo::load(&ctx).await?;
        let signer = ctx.signing_key()?;
        let socket = tokio::time::timeout(
            Duration::from_secs(30),
            ctx.client
                .websocket(websocket_url(registry)?, response_config()),
        )
        .await
        .with_kind(ErrorKind::Network)??;
        (socket, device, signer)
    };
    receive_updates(socket, db, subscriptions, &device, &signer, backoff).await
}

struct WatchState {
    subscription: Subscription,
    dump: Option<Dump>,
}

fn update_dump(state: &mut WatchState, message: Value) -> Result<bool, Error> {
    if message.get("rpcstream").is_some() {
        let event: RpcStreamResponse = from_value(message)?;
        let result = event.result?;
        if event.end {
            return Ok(false);
        }
        let dump = state
            .dump
            .as_mut()
            .or_not_found("registry initial response")?;
        let revision: Revision = from_value(result)?;
        ensure_code!(
            revision.id > dump.id,
            ErrorKind::Database,
            "out-of-order registry revision"
        );
        patch_db::json_patch::patch(&mut dump.value, &revision.patch)
            .with_kind(ErrorKind::Database)?;
        dump.id = revision.id;
    } else {
        ensure_code!(
            state.dump.is_none(),
            ErrorKind::Network,
            "duplicate registry initial response"
        );
        let response: RpcResponse = from_value(message)?;
        state.dump = Some(from_value(response.result?)?);
    }
    Ok(true)
}

pub(super) async fn receive_updates<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: WebSocketStream<S>,
    db: &TypedPatchDb<Database>,
    subscriptions: &BTreeSet<Subscription>,
    device: &DeviceInfo,
    signer: &Signer,
    backoff: &mut u64,
) -> Result<(), Error> {
    let mut watches = BTreeMap::new();
    for subscription in subscriptions {
        let id = subscription.request_id();
        let request = subscription_request(subscription, &device.os.version, signer).await?;
        tokio::time::timeout(
            Duration::from_secs(15),
            socket.send(Message::Text(request.to_string().into())),
        )
        .await
        .with_kind(ErrorKind::Network)?
        .with_kind(ErrorKind::Network)?;
        watches.insert(
            id,
            WatchState {
                subscription: subscription.clone(),
                dump: None,
            },
        );
    }
    let initial_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
    heartbeat.tick().await;
    let mut last_received = tokio::time::Instant::now();
    loop {
        tokio::select! {
            message = socket.next() => {
                last_received = tokio::time::Instant::now();
                match message.transpose().with_kind(ErrorKind::Network)? {
                    Some(Message::Text(text)) => {
                        let message: Value = serde_json::from_str(&text).with_kind(ErrorKind::Deserialization)?;
                        let id = message.get("id").and_then(Value::as_u64).or_not_found("registry response id")?;
                        let state = watches.get_mut(&id).or_not_found("registry watch")?;
                        if !update_dump(state, message)? { return Ok(()); }
                        publish(db, &state.subscription, &state.dump.as_ref().unwrap().value, device).await?;
                        *backoff = 30;
                    }
                    Some(Message::Ping(_)) => {
                        tokio::time::timeout(Duration::from_secs(15), socket.flush())
                            .await.with_kind(ErrorKind::Network)?.with_kind(ErrorKind::Network)?;
                    }
                    Some(Message::Binary(_)) => return Err(Error::new(eyre!("expected a JSON registry response"), ErrorKind::Network)),
                    Some(Message::Close(_)) | None => return Ok(()),
                    _ => (),
                }
            }
            _ = heartbeat.tick() => {
                tokio::time::timeout(Duration::from_secs(15), socket.send(Message::Ping(Default::default())))
                    .await.with_kind(ErrorKind::Network)?.with_kind(ErrorKind::Network)?;
            }
            _ = tokio::time::sleep_until(last_received + Duration::from_secs(120)) => {
                return Err(Error::new(eyre!("registry heartbeat timed out"), ErrorKind::Network));
            }
            _ = tokio::time::sleep_until(initial_deadline), if watches.values().any(|state| state.dump.is_none()) => {
                return Err(Error::new(eyre!("registry initial response timed out"), ErrorKind::Network));
            }
        }
    }
}
