//! A registry configured with `mirror-of` follows another registry's `/index`.
//! The mirror stores the index the upstream shows to the mirror's server identity.
//! Admins, metrics and the server identity stay local.
//! Index-writing RPCs are rejected until the setting is removed and the daemon restarts.
//! Icon download failures do not prevent index synchronization.
//! Public-only subscriptions require a restart after private-access grants.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use http::HeaderMap;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;
use url::Url;

use crate::middleware::auth::session::{RpcSessionUpgrader, client as session_client};
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::package::icon::{copy_icon, referenced_icons};
use crate::registry::package::index::PackageIndex;
use crate::registry::sync::connection::{SubscriptionFollower, db_subscribe_request};
use crate::registry::sync::{INITIAL_BACKOFF, next_retry};
use crate::registry::{FullIndex, RegistryDatabase};
use crate::sign::Signer;
use crate::sign::session::Target;
use crate::util::future::NonDetachingJoinHandle;

/// RPC metadata marking a handler that writes the registry index.
pub const WRITES_INDEX: &str = "writes_index";

const ICON_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Upstream {
    pub url: Url,
    _sync: NonDetachingJoinHandle<()>,
}

/// Clients the mirror uses to reach its upstream.
pub(crate) struct UpstreamClients {
    pub http: reqwest::Client,
    /// HTTP/1-only and non-redirecting, as WebSocket upgrades require.
    pub websocket: reqwest::Client,
}

impl UpstreamClients {
    pub(crate) fn new(tor_proxy_url: Url) -> Result<Self, Error> {
        let builder = || {
            super::context::client_builder(tor_proxy_url.clone())
                .redirect(reqwest::redirect::Policy::none())
        };
        Ok(Self {
            http: builder()
                .timeout(ICON_DOWNLOAD_TIMEOUT)
                .build()
                .with_kind(ErrorKind::ParseUrl)?,
            websocket: builder()
                .http1_only()
                .build()
                .with_kind(ErrorKind::ParseUrl)?,
        })
    }
}

impl Upstream {
    pub(crate) fn spawn(
        db: TypedPatchDb<RegistryDatabase>,
        icons: PathBuf,
        clients: UpstreamClients,
        signer: Signer,
        url: Url,
    ) -> Self {
        let sync = {
            let url = url.clone();
            tokio::spawn(async move {
                match signer.verifying_key().await {
                    Ok(key) => tracing::info!(
                        %url,
                        key = %key.interned_der64(),
                        "Mirroring upstream registry"
                    ),
                    Err(error) => tracing::warn!("Could not read the mirror identity: {error}"),
                }
                let mut backoff = INITIAL_BACKOFF;
                loop {
                    match follow(&db, &icons, &clients, &signer, &url, &mut backoff).await {
                        Ok(()) => {
                            tracing::warn!(%url, "Upstream registry closed the index subscription")
                        }
                        Err(error) => {
                            tracing::error!(%url, "Could not mirror upstream registry: {error}");
                            tracing::debug!("{error:?}");
                        }
                    }
                    tokio::time::sleep(next_retry(&mut backoff)).await;
                }
            })
        };
        Self {
            url,
            _sync: sync.into(),
        }
    }
}

async fn follow(
    db: &TypedPatchDb<RegistryDatabase>,
    icons: &Path,
    clients: &UpstreamClients,
    signer: &Signer,
    upstream: &Url,
    backoff: &mut u64,
) -> Result<(), Error> {
    let address = upstream.host_str().or_not_found("registry hostname")?;
    let (_, socket) = session_client::connect(
        &clients.websocket,
        signer,
        RpcSessionUpgrader::url(upstream.clone())?,
        address,
        Target::Rpc,
        HeaderMap::new(),
    )
    .await?;
    let mut follower = SubscriptionFollower::start(
        socket,
        BTreeMap::from([(1, db_subscribe_request(1, "/index", None))]),
    )
    .await?;
    while let Some((_, index)) = follower.next().await? {
        replace_index(db, icons, &clients.http, signer, upstream, &index).await?;
        *backoff = INITIAL_BACKOFF;
    }
    Ok(())
}

/// Stores the upstream index after copying the stored icons it refers to.
/// An icon the upstream cannot supply is skipped and retried on the next revision.
pub(crate) async fn replace_index(
    db: &TypedPatchDb<RegistryDatabase>,
    icons: &Path,
    http: &reqwest::Client,
    signer: &Signer,
    upstream: &Url,
    index: &Value,
) -> Result<(), Error> {
    let index: FullIndex = from_value(index.clone())?;
    let packages = Model::<PackageIndex>::new(&index.package)?;
    for path in referenced_icons(&packages)? {
        if let Err(error) = copy_icon(http, signer, upstream, icons, &path).await {
            tracing::warn!(%upstream, %path, "Could not copy upstream icon: {error}");
            tracing::debug!("{error:?}");
        }
    }
    db.mutate(|db| db.as_index_mut().ser(&index)).await.result
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default)]
    writes_index: bool,
}

/// Rejects calls tagged with `WRITES_INDEX` while the registry mirrors an upstream.
#[derive(Clone, Default)]
pub struct MirrorGuard;

impl Middleware<RegistryContext> for MirrorGuard {
    type Metadata = Metadata;
    async fn process_rpc_request(
        &mut self,
        ctx: &RegistryContext,
        metadata: Self::Metadata,
        _: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        match &ctx.upstream {
            Some(upstream) if metadata.writes_index => {
                Err(RpcResponse::from_result(Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "registry.mirror.read-only",
                            upstream = upstream.url.as_str()
                        )
                    ),
                    ErrorKind::InvalidRequest,
                ))))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
