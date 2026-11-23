use std::num::NonZeroUsize;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use patch_db::{Dump, Revision};
use rpc_toolkit::{RPCSTREAM_VERSION, RpcResponse, RpcStreamResponse};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::*;
use crate::context::rpc::ReloadableHttpClient;
use crate::middleware::auth::session::{RpcSessionUpgrader, client as session_client};
use crate::middleware::auth::signature::SigningContext;
use crate::sign::Signer;
use crate::sign::session::Target;
use crate::util::net::secure::{
    DEFAULT_RPC_RESPONSE_LIMIT as MAX_RESPONSE_SIZE, EncryptedSocket, wire_message_size,
};
use crate::util::net::tungstenite::transport;

pub(super) fn websocket_url(registry: &Url) -> Result<Url, Error> {
    RpcSessionUpgrader::url(registry.clone())
}

pub(super) fn response_config() -> WebSocketConfig {
    let wire_limit = wire_message_size(NonZeroUsize::new(MAX_RESPONSE_SIZE).unwrap()).get();
    WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(4096)
        .max_message_size(Some(wire_limit))
        .max_frame_size(Some(wire_limit))
}

pub(super) fn subscription_request(
    subscription: &Subscription,
    view: &RegistryView,
    since: Option<&DbCursor>,
) -> Value {
    let mut request = db_subscribe_request(
        subscription.request_id(),
        subscription.pointer(),
        Some(&view.os_version),
    );
    if let Some(language) = &view.language {
        request["params"]["language"] = imbl_value::json!(language);
    }
    if let Some(since) = since {
        request["params"]["since"] = imbl_value::json!(since);
    }
    request
}

/// Streams the registry subtree at the pointer.
/// Omitting the StartOS version selects the canonical index format.
pub(crate) fn db_subscribe_request(
    id: u64,
    pointer: &str,
    os_version: Option<&exver::Version>,
) -> Value {
    let mut params = imbl_value::json!({ "pointer": pointer });
    if let Some(os_version) = os_version {
        params["osVersion"] = imbl_value::json!(os_version);
    }
    imbl_value::json!({
        "jsonrpc": "2.0", "rpcstream": RPCSTREAM_VERSION, "id": id,
        "method": "db.subscribe", "params": params
    })
}

/// Recognizes legacy non-upgrade responses, never rejected encrypted admission.
/// Legacy JSON-RPC dispatchers return HTTP 200 for rejected upgrades.
pub(crate) fn websocket_unsupported(error: &Error) -> bool {
    use tokio_tungstenite::tungstenite::Error as WebSocketError;
    matches!(
        error.source.downcast_ref::<WebSocketError>(),
        Some(WebSocketError::Http(response))
            if matches!(response.status().as_u16(), 200 | 404 | 405 | 501)
    )
}

pub(crate) async fn connect(
    client: &ReloadableHttpClient,
    registry: &Url,
    signer: &Signer,
    headers: http::HeaderMap,
) -> Result<EncryptedSocket, Error> {
    let address = registry.host_str().or_not_found("registry hostname")?;
    let url = websocket_url(registry)?;
    let mut request = url.into_client_request().with_kind(ErrorKind::Network)?;
    request.headers_mut().extend(headers);
    session_client::admit(signer, address, Target::Rpc, async {
        let socket = client.websocket(request, response_config()).await?;
        Ok(transport(socket))
    })
    .await
    .map(|(_, socket)| socket)
}

pub(super) async fn subscribe(
    db: &TypedPatchDb<Database>,
    subscriptions: &BTreeSet<Subscription>,
    context: &ContextProvider,
    backoff: &mut u64,
) -> Result<(), Error> {
    let (socket, device) = {
        let Some(ctx) = context() else { return Ok(()) };
        let registry = subscriptions.first().unwrap().registry();
        let device = DeviceInfo::load(&ctx).await?;
        let signer = ctx.signing_key()?;
        let socket = connect(&ctx.client, registry, &signer, http::HeaderMap::new()).await?;
        (socket, device)
    };
    receive_updates(socket, db, subscriptions, &device, backoff).await
}

/// A subscription's first response: the subtree, or the revisions after the requested cursor.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubscriptionStart {
    /// Absent from registries that cannot resume.
    #[serde(default)]
    pub epoch: Option<InternedString>,
    pub id: u64,
    #[serde(default)]
    pub value: OrdValue,
    /// Present only when the registry resumed from the requested cursor.
    #[serde(default)]
    pub revisions: Option<Vec<Revision>>,
}

#[derive(Debug)]
pub(crate) enum SubscriptionEvent {
    Start(SubscriptionStart),
    Revision(Revision),
}

/// A subscription's subtree, kept current by applying each event.
#[derive(Default)]
pub(crate) struct TrackedValue(Option<Dump>);

impl TrackedValue {
    pub(crate) fn apply(&mut self, event: SubscriptionEvent) -> Result<&OrdValue, Error> {
        match event {
            SubscriptionEvent::Start(start) => {
                ensure_code!(
                    start.revisions.is_none(),
                    ErrorKind::Network,
                    "{}",
                    t!("registry.sync.unrequested-replay")
                );
                self.0 = Some(Dump {
                    id: start.id,
                    value: start.value,
                });
            }
            SubscriptionEvent::Revision(revision) => {
                let dump = self.0.as_mut().or_not_found("registry initial response")?;
                ensure_code!(
                    revision.id > dump.id,
                    ErrorKind::Database,
                    "out-of-order registry revision"
                );
                patch_db::json_patch::patch(&mut dump.value, &revision.patch)
                    .with_kind(ErrorKind::Database)?;
                dump.id = revision.id;
            }
        }
        Ok(&self.0.as_ref().unwrap().value)
    }
}

#[derive(Default)]
struct WatchState {
    started: bool,
    tracked: TrackedValue,
}

/// `None` once the registry ends the subscription.
fn subscription_event(
    state: &mut WatchState,
    message: Value,
) -> Result<Option<SubscriptionEvent>, Error> {
    if message.get("rpcstream").is_some() {
        let event: RpcStreamResponse = from_value(message)?;
        let result = event.result?;
        if event.end {
            return Ok(None);
        }
        ensure_code!(
            state.started,
            ErrorKind::Network,
            "{}",
            t!("registry.sync.revision-before-start")
        );
        Ok(Some(SubscriptionEvent::Revision(from_value(result)?)))
    } else {
        ensure_code!(
            !state.started,
            ErrorKind::Network,
            "duplicate registry initial response"
        );
        let response: RpcResponse = from_value(message)?;
        let start = from_value(response.result?)?;
        state.started = true;
        Ok(Some(SubscriptionEvent::Start(start)))
    }
}

/// Failed initial responses discard resume cursors but retain cached indexes.
pub(super) async fn receive_updates(
    socket: EncryptedSocket,
    db: &TypedPatchDb<Database>,
    subscriptions: &BTreeSet<Subscription>,
    device: &DeviceInfo,
    backoff: &mut u64,
) -> Result<(), Error> {
    let by_id: BTreeMap<u64, &Subscription> = subscriptions
        .iter()
        .map(|subscription| (subscription.request_id(), subscription))
        .collect();
    let view = RegistryView::new(device);
    let saved = db.peek().await;
    let requests = subscriptions
        .iter()
        .map(|subscription| {
            let since = match subscription {
                Subscription::Index(url) => resumable_cursor(&saved, url, &view)?,
                Subscription::Os(_) => None,
            };
            Ok((
                subscription.request_id(),
                subscription_request(subscription, &view, since.as_ref()),
            ))
        })
        .collect::<Result<_, Error>>()?;
    drop(saved);
    let mut follower = SubscriptionFollower::start(socket, requests).await?;
    let mut os = TrackedValue::default();
    let result = async {
        while let Some((id, event)) = follower.next_event().await? {
            let subscription = by_id[&id];
            match subscription {
                Subscription::Index(url) => ingest(db, url, &view, &event, device).await?,
                Subscription::Os(_) => publish(db, subscription, os.apply(event)?, device).await?,
            }
            *backoff = INITIAL_BACKOFF;
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        for (&id, &subscription) in &by_id {
            if let Subscription::Index(url) = subscription
                && !follower.watches[&id].started
            {
                let _ = db.mutate(|db| forget_cursor(db, url)).await.result;
            }
        }
    }
    result
}

/// Follows `db.subscribe` streams on one admitted socket.
pub(crate) struct SubscriptionFollower {
    socket: EncryptedSocket,
    watches: BTreeMap<u64, WatchState>,
    initial_deadline: tokio::time::Instant,
    heartbeat: tokio::time::Interval,
    last_received: tokio::time::Instant,
}

impl SubscriptionFollower {
    /// Sends each request; the key is its JSON-RPC ID.
    pub(crate) async fn start(
        mut socket: EncryptedSocket,
        requests: BTreeMap<u64, Value>,
    ) -> Result<Self, Error> {
        let mut watches = BTreeMap::new();
        for (id, request) in requests {
            tokio::time::timeout(
                Duration::from_secs(15),
                socket.send(Message::Text(request.to_string().into())),
            )
            .await
            .with_kind(ErrorKind::Network)?
            .with_kind(ErrorKind::Network)?;
            watches.insert(id, WatchState::default());
        }
        let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
        heartbeat.tick().await;
        Ok(Self {
            socket,
            watches,
            initial_deadline: tokio::time::Instant::now() + Duration::from_secs(30),
            heartbeat,
            last_received: tokio::time::Instant::now(),
        })
    }

    /// Returns the request ID and full subtree after each dump or revision.
    /// `None` once the registry ends a subscription or closes the socket.
    pub(crate) async fn next(&mut self) -> Result<Option<(u64, OrdValue)>, Error> {
        let Some((id, event)) = self.next_event().await? else {
            return Ok(None);
        };
        let state = self.watches.get_mut(&id).or_not_found("registry watch")?;
        Ok(Some((id, state.tracked.apply(event)?.clone())))
    }

    /// Returns the request ID and each start or revision as received.
    /// `None` once the registry ends a subscription or closes the socket.
    pub(crate) async fn next_event(&mut self) -> Result<Option<(u64, SubscriptionEvent)>, Error> {
        loop {
            tokio::select! {
                message = self.socket.next() => {
                    self.last_received = tokio::time::Instant::now();
                    match message.transpose().with_kind(ErrorKind::Network)? {
                        Some(Message::Text(text)) => {
                            let message: Value = serde_json::from_str(&text).with_kind(ErrorKind::Deserialization)?;
                            let id = message.get("id").and_then(Value::as_u64).or_not_found("registry response id")?;
                            let state = self.watches.get_mut(&id).or_not_found("registry watch")?;
                            return Ok(subscription_event(state, message)?.map(|event| (id, event)));
                        }
                        Some(Message::Ping(_)) => {
                            tokio::time::timeout(Duration::from_secs(15), self.socket.flush())
                                .await.with_kind(ErrorKind::Network)?.with_kind(ErrorKind::Network)?;
                        }
                        Some(Message::Binary(_)) => return Err(Error::new(eyre!("expected a JSON registry response"), ErrorKind::Network)),
                        Some(Message::Close(_)) | None => return Ok(None),
                        _ => (),
                    }
                }
                _ = self.heartbeat.tick() => {
                    tokio::time::timeout(Duration::from_secs(15), self.socket.send(Message::Ping(Default::default())))
                        .await.with_kind(ErrorKind::Network)?.with_kind(ErrorKind::Network)?;
                }
                _ = tokio::time::sleep_until(self.last_received + Duration::from_secs(120)) => {
                    return Err(Error::new(eyre!("registry heartbeat timed out"), ErrorKind::Network));
                }
                _ = tokio::time::sleep_until(self.initial_deadline), if self.watches.values().any(|state| !state.started) => {
                    return Err(Error::new(eyre!("registry initial response timed out"), ErrorKind::Network));
                }
            }
        }
    }
}
