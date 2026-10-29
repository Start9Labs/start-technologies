use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use futures::{StreamExt, stream};
use josekit::jwk::{Jwk, P_256};
use josekit::jws::{ES256, JwsHeader};
use josekit::jwt::JwtPayload;
use reqwest::{Client, StatusCode};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;
use web_push_native::{Auth, WebPushBuilder, p256};

use crate::context::RpcContext;
use crate::db::model::{Database, DatabaseModel};
use crate::prelude::*;
use crate::util::new_guid;

const MAX_PENDING: usize = 512;
const MAX_AGE: i64 = 86400;
const RETRY_SECONDS: i64 = 60;
const MAX_PAYLOAD: usize = 3993;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PushKeys {
    /// Base64url-encoded uncompressed P-256 public key.
    pub p256dh: String,
    /// Base64url-encoded 16-byte authentication secret.
    pub auth: String,
}
rpc_toolkit::reflect_ts!(PushKeys);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PushSubscription {
    /// HTTP(S) push resource selected by the browser, including LAN endpoints.
    pub endpoint: String,
    pub keys: PushKeys,
}
rpc_toolkit::reflect_ts!(PushSubscription);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct EndpointParams {
    pub endpoint: String,
}
rpc_toolkit::reflect_ts!(EndpointParams);

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushState {
    vapid_private_key: Option<Jwk>,
    subscriptions: Vec<StoredSubscription>,
    pending: Vec<PendingPush>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSubscription {
    id: String,
    subscription: PushSubscription,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingPush {
    id: String,
    subscription_id: String,
    created_at: DateTime<Utc>,
    retry_at: DateTime<Utc>,
    payload: String,
}

fn error(key: &'static str, kind: ErrorKind) -> Error {
    Error::new(eyre!("{}", t!(key)), kind)
}

fn crypto_error() -> Error {
    error("notifications.push.crypto-failed", ErrorKind::Unknown)
}

pub(super) fn push<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "key",
            from_fn_async(key)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand("list", from_fn_async(list).no_cli())
        .subcommand(
            "subscribe",
            from_fn_async(subscribe)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "unsubscribe",
            from_fn_async(unsubscribe)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "test",
            from_fn_async(test)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
}

fn mutate_state<T>(
    db: &mut DatabaseModel,
    f: impl FnOnce(&mut PushState) -> Result<T, Error>,
) -> Result<T, Error> {
    db.as_private_mut().as_push_mut().mutate(f)
}

impl PushState {
    fn private_key(&mut self) -> Result<&Jwk, Error> {
        if self.vapid_private_key.is_none() {
            self.vapid_private_key = Some(Jwk::generate_ec_key(P_256).map_err(|_| crypto_error())?);
        }
        Ok(self.vapid_private_key.as_ref().unwrap())
    }

    fn public_key(&mut self) -> Result<String, Error> {
        vapid_public_key(self.private_key()?)
    }

    fn upsert(&mut self, subscription: PushSubscription) {
        if self
            .subscriptions
            .iter()
            .any(|s| s.subscription == subscription)
        {
            return;
        }
        self.remove_endpoint(&subscription.endpoint);
        self.subscriptions.push(StoredSubscription {
            id: new_guid().to_string(),
            subscription,
        });
    }

    fn remove_endpoint(&mut self, endpoint: &str) {
        self.subscriptions
            .retain(|s| s.subscription.endpoint != endpoint);
        self.prune(Utc::now());
    }

    fn prune(&mut self, now: DateTime<Utc>) {
        self.pending.retain(|p| {
            (now - p.created_at).num_seconds() < MAX_AGE
                && self.subscriptions.iter().any(|s| s.id == p.subscription_id)
        });
        let excess = self.pending.len().saturating_sub(MAX_PENDING);
        self.pending.drain(..excess);
    }

    fn enqueue(&mut self, title: &str, message: &str, now: DateTime<Utc>) -> Result<(), Error> {
        self.prune(now);
        if self.subscriptions.is_empty() {
            return Ok(());
        }
        let payload = payload(title, message)?;
        for subscription in &self.subscriptions {
            self.pending.push(PendingPush {
                id: new_guid().to_string(),
                subscription_id: subscription.id.clone(),
                created_at: now,
                retry_at: now,
                payload: payload.clone(),
            });
        }
        self.prune(now);
        Ok(())
    }

    fn complete(&mut self, pending: &PendingPush, outcome: Delivery, now: DateTime<Utc>) {
        if outcome == Delivery::Gone {
            self.subscriptions
                .retain(|s| s.id != pending.subscription_id);
        }
        if let Some(p) = self.pending.iter_mut().find(|p| p.id == pending.id) {
            if outcome == Delivery::Retry {
                p.retry_at = now + chrono::Duration::seconds(RETRY_SECONDS);
            } else {
                self.pending.retain(|p| p.id != pending.id);
            }
        }
        self.prune(now);
    }
}

pub(super) fn enqueue(db: &mut DatabaseModel, title: &str, message: &str) -> Result<(), Error> {
    mutate_state(db, |state| state.enqueue(title, message, Utc::now()))
}

fn payload(title: &str, body: &str) -> Result<String, Error> {
    let mut title = title.to_owned();
    let mut body = body.to_owned();
    let tag = new_guid();
    loop {
        let payload = serde_json::to_string(&serde_json::json!({
            "notification": {
                "title": if title.is_empty() { "StartOS" } else { &title },
                "body": body,
                "tag": tag,
                "data": {"onActionClick": {"default": {
                    "operation": "navigateLastFocusedOrOpen", "url": "notifications"
                }}}
            }
        }))
        .with_kind(ErrorKind::Serialization)?;
        if payload.len() <= MAX_PAYLOAD {
            return Ok(payload);
        }
        let text = if body.len() >= title.len() {
            &mut body
        } else {
            &mut title
        };
        let mut keep = text
            .len()
            .saturating_sub((payload.len() - MAX_PAYLOAD).max(1));
        while !text.is_char_boundary(keep) {
            keep -= 1;
        }
        text.truncate(keep);
    }
}

fn canonical_endpoint(endpoint: &str) -> Result<String, Error> {
    let invalid = || {
        error(
            "notifications.push.invalid-endpoint",
            ErrorKind::InvalidRequest,
        )
    };
    let url = url::Url::parse(endpoint).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid());
    }
    Ok(url.to_string())
}

fn decode_keys(keys: &PushKeys) -> Result<(p256::PublicKey, Auth), Error> {
    let invalid = || error("notifications.push.invalid-keys", ErrorKind::InvalidRequest);
    let key = URL_SAFE_NO_PAD
        .decode(&keys.p256dh)
        .map_err(|_| invalid())?;
    let auth: [u8; 16] = URL_SAFE_NO_PAD
        .decode(&keys.auth)
        .map_err(|_| invalid())?
        .try_into()
        .map_err(|_| invalid())?;
    if key.len() != 65 {
        return Err(invalid());
    }
    let key = p256::PublicKey::from_sec1_bytes(&key).map_err(|_| invalid())?;
    Ok((key, auth.into()))
}

fn validate(subscription: &mut PushSubscription) -> Result<(), Error> {
    subscription.endpoint = canonical_endpoint(&subscription.endpoint)?;
    decode_keys(&subscription.keys)?;
    Ok(())
}

async fn key(ctx: RpcContext) -> Result<String, Error> {
    ctx.db
        .mutate(|db| mutate_state(db, PushState::public_key))
        .await
        .result
}

async fn list(ctx: RpcContext) -> Result<Vec<PushSubscription>, Error> {
    Ok(ctx
        .db
        .peek()
        .await
        .as_private()
        .as_push()
        .de()?
        .subscriptions
        .into_iter()
        .map(|s| s.subscription)
        .collect())
}

async fn subscribe(ctx: RpcContext, mut subscription: PushSubscription) -> Result<(), Error> {
    validate(&mut subscription)?;
    ctx.db
        .mutate(|db| {
            mutate_state(db, |state| {
                state.private_key()?;
                state.upsert(subscription);
                Ok(())
            })
        })
        .await
        .result
}

async fn unsubscribe(
    ctx: RpcContext,
    EndpointParams { endpoint }: EndpointParams,
) -> Result<(), Error> {
    let endpoint = canonical_endpoint(&endpoint)?;
    ctx.db
        .mutate(|db| {
            mutate_state(db, |state| {
                state.remove_endpoint(&endpoint);
                Ok(())
            })
        })
        .await
        .result
}

async fn test(ctx: RpcContext, EndpointParams { endpoint }: EndpointParams) -> Result<(), Error> {
    test_subscription(&ctx.db, &ctx.client.get(), &endpoint).await
}

async fn test_subscription(
    db: &TypedPatchDb<Database>,
    client: &Client,
    endpoint: &str,
) -> Result<(), Error> {
    let endpoint = canonical_endpoint(endpoint)?;
    let state = db.peek().await.as_private().as_push().de()?;
    let subscription = state
        .subscriptions
        .iter()
        .find(|s| s.subscription.endpoint == endpoint)
        .ok_or_else(|| error("notifications.push.not-found", ErrorKind::NotFound))?;
    let payload = payload(
        &t!("notifications.push.test-title"),
        &t!("notifications.push.test-body"),
    )?;
    let outcome = deliver(
        client,
        subscription,
        state.vapid_private_key.as_ref(),
        &payload,
        MAX_AGE as u32,
    )
    .await;
    if outcome == Delivery::Gone {
        db.mutate(|db| {
            mutate_state(db, |state| {
                state.subscriptions.retain(|s| s.id != subscription.id);
                state.prune(Utc::now());
                Ok(())
            })
        })
        .await
        .result?;
    }
    if outcome != Delivery::Delivered {
        return Err(error(
            "notifications.push.delivery-failed",
            ErrorKind::Network,
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    Delivered,
    Gone,
    Retry,
    Rejected,
}

fn vapid_public_key(key: &Jwk) -> Result<String, Error> {
    let scalar = key
        .parameter("d")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(crypto_error)?;
    let scalar = URL_SAFE_NO_PAD.decode(scalar).map_err(|_| crypto_error())?;
    let key = p256::SecretKey::from_slice(&scalar).map_err(|_| crypto_error())?;
    Ok(URL_SAFE_NO_PAD.encode(key.public_key().to_encoded_point(false).as_bytes()))
}

fn vapid_authorization(key: &Jwk, endpoint: &str) -> Result<String, Error> {
    let origin = url::Url::parse(endpoint)
        .map_err(|_| crypto_error())?
        .origin();
    let mut payload = JwtPayload::new();
    payload
        .set_claim(
            "aud",
            Some(serde_json::Value::String(origin.ascii_serialization())),
        )
        .map_err(|_| crypto_error())?;
    payload.set_subject("mailto:support@start9.com");
    payload
        .set_claim(
            "exp",
            Some(serde_json::json!(Utc::now().timestamp() + 12 * 3600)),
        )
        .map_err(|_| crypto_error())?;
    let mut header = JwsHeader::new();
    header.set_token_type("JWT");
    let signer = ES256.signer_from_jwk(key).map_err(|_| crypto_error())?;
    let token =
        josekit::jwt::encode_with_signer(&payload, &header, &signer).map_err(|_| crypto_error())?;
    Ok(format!("vapid t={token}, k={}", vapid_public_key(key)?))
}

fn web_push_request(
    client: &Client,
    subscription: &PushSubscription,
    private_key: &Jwk,
    payload: &str,
    ttl: u32,
) -> Result<reqwest::RequestBuilder, Error> {
    let (key, auth) = decode_keys(&subscription.keys)?;
    let message = WebPushBuilder::new(
        subscription.endpoint.parse().map_err(|_| crypto_error())?,
        key,
        auth,
    )
    .with_valid_duration(Duration::from_secs(ttl.into()));
    let request = message
        .build(payload.as_bytes())
        .map_err(|_| crypto_error())?;
    let (parts, body) = request.into_parts();
    // The library's VAPID helper omits non-default ports from its audience.
    Ok(client
        .request(parts.method, &subscription.endpoint)
        .headers(parts.headers)
        .header(
            "Authorization",
            vapid_authorization(private_key, &subscription.endpoint)?,
        )
        .body(body))
}

async fn deliver(
    client: &Client,
    subscription: &StoredSubscription,
    private_key: Option<&Jwk>,
    payload: &str,
    ttl: u32,
) -> Delivery {
    let subscription = &subscription.subscription;
    let request = match private_key
        .and_then(|key| web_push_request(client, subscription, key, payload, ttl).ok())
    {
        Some(request) => request,
        None => return Delivery::Rejected,
    };
    let response = match request.timeout(Duration::from_secs(30)).send().await {
        Ok(response) => response,
        Err(_) => return Delivery::Retry,
    };
    let status = response.status();
    if status.is_success() {
        Delivery::Delivered
    } else if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE) {
        Delivery::Gone
    } else if status.is_server_error()
        || matches!(
            status,
            StatusCode::REQUEST_TIMEOUT
                | StatusCode::TOO_MANY_REQUESTS
                | StatusCode::UNAUTHORIZED
                | StatusCode::FORBIDDEN
        )
    {
        Delivery::Retry
    } else {
        Delivery::Rejected
    }
}

pub(crate) async fn drain(db: &TypedPatchDb<Database>, client: &Client) -> Result<(), Error> {
    let mut state = db.peek().await.as_private().as_push().de()?;
    let now = Utc::now();
    let before = state.pending.len();
    state.prune(now);
    if before != state.pending.len() {
        db.mutate(|db| {
            mutate_state(db, |state| {
                state.prune(now);
                Ok(())
            })
        })
        .await
        .result?;
    }
    let jobs: Vec<_> = state
        .pending
        .iter()
        .filter(|p| p.retry_at <= now)
        .filter_map(|p| {
            state
                .subscriptions
                .iter()
                .find(|s| s.id == p.subscription_id)
                .map(|s| (p.clone(), s.clone()))
        })
        .collect();
    let mut deliveries = stream::iter(jobs.into_iter().map(|(pending, subscription)| {
        let key = state.vapid_private_key.as_ref();
        async move {
            let ttl = MAX_AGE - (Utc::now() - pending.created_at).num_seconds();
            let outcome = if ttl <= 0 {
                Delivery::Rejected
            } else {
                deliver(
                    client,
                    &subscription,
                    key,
                    &pending.payload,
                    ttl.min(MAX_AGE) as u32,
                )
                .await
            };
            (pending, outcome)
        }
    }))
    .buffer_unordered(8);
    while let Some((pending, outcome)) = deliveries.next().await {
        db.mutate(|db| {
            mutate_state(db, |state| {
                state.complete(&pending, outcome, Utc::now());
                Ok(())
            })
        })
        .await
        .result?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
