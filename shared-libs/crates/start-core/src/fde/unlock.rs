use std::ops::Deref;
use std::sync::Arc;

use axum::Router;
use axum::http::{Extensions, Request, StatusCode};
use axum::response::IntoResponse;
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerExt, HttpServer, ParentHandler, Server, from_fn, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, oneshot};
use zeroize::Zeroizing;

use super::initramfs::invalid_handoff;
use super::network::UnlockNetwork;
use crate::auth::PasswordType;
use crate::context::config::ServerConfig;
use crate::context::rpc::InitRpcContextPhases;
use crate::context::setup::CURRENT_SECRET;
use crate::context::startup::StartupRpc;
use crate::context::{InitContext, RpcContext};
use crate::disk::fde::LuksPool;
use crate::disk::fsck::{RepairStrategy, RequiresReboot};
use crate::init::InitPhases;
use crate::middleware::auth::session::{SecureSession, SessionContext};
use crate::middleware::auth::signature::{
    NonceCache, RequestReplayCheck, SignatureAuthContext, startos_signing_identities,
};
use crate::middleware::json::RequireJson;
use crate::net::dns::DnsController;
use crate::net::gateway::WildcardListener;
use crate::net::host::Host;
use crate::net::request_origin::RequestOrigin;
use crate::net::static_server::{EMPTY_DIR, UI_CELL, UiContext};
use crate::net::web_server::WebServer;
use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::rpc_continuations::RpcContinuations;
use crate::shutdown::Shutdown;
use crate::util::future::complete_owned_operation;
use crate::util::sync::Watch;
use crate::{ApiState, DATA_DIR};

pub struct UnlockContextSeed {
    startup_rpc: StartupRpc,
    host: Watch<Model<Host>>,
    disk_guid: InternedString,
    progress: FullProgressTracker,
    completion: Mutex<Option<oneshot::Sender<RequiresReboot>>>,
    rpc_continuations: RpcContinuations,
}

#[derive(Clone)]
pub struct UnlockContext(Arc<UnlockContextSeed>, Option<SecureSession>);

impl Deref for UnlockContext {
    type Target = UnlockContextSeed;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Context for UnlockContext {}
impl UnlockContext {
    fn new(
        host: Watch<Model<Host>>,
        disk_guid: InternedString,
        progress: FullProgressTracker,
    ) -> (Self, oneshot::Receiver<RequiresReboot>) {
        let (completion, receiver) = oneshot::channel();
        (
            Self(
                Arc::new(UnlockContextSeed {
                    startup_rpc: StartupRpc::new(),
                    host,
                    disk_guid,
                    progress,
                    completion: Mutex::new(Some(completion)),
                    rpc_continuations: RpcContinuations::new(),
                }),
                None,
            ),
            receiver,
        )
    }
}
impl AsRef<RpcContinuations> for UnlockContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}
impl AsRef<josekit::jwk::Jwk> for UnlockContext {
    fn as_ref(&self) -> &josekit::jwk::Jwk {
        &*CURRENT_SECRET
    }
}
impl SessionContext for UnlockContext {
    fn secure_session(&self) -> Option<&SecureSession> {
        self.1.as_ref()
    }
}
impl SignatureAuthContext for UnlockContext {
    fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
        Ok(self.startup_rpc.identity.clone())
    }
    fn replay_check(&self) -> RequestReplayCheck {
        RequestReplayCheck::new(&self.0, |seed| Some(Self(seed, None)))
    }
    fn with_secure_session(self, session: SecureSession) -> Self {
        Self(self.0, Some(session))
    }
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
        self.startup_rpc.nonces.mutate(f)
    }
    async fn clock_synced(&self) -> bool {
        StartupRpc::clock_synced().await
    }
    async fn sig_context(
        &self,
        extensions: &Extensions,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        let origin = RequestOrigin::from_extensions(extensions);
        let binding = self
            .host
            .read()
            .as_bindings()
            .as_idx(&80)
            .map(|binding| binding.de())
            .transpose();
        std::iter::once(
            binding.map(|binding| startos_signing_identities(binding.as_ref(), &origin)),
        )
        .flatten_ok()
    }
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnlockParams {
    pub password: PasswordType,
}
rpc_toolkit::reflect_ts!(UnlockParams);

/// Pre-unlock authority excludes initialization and management operations.
/// Accepted unlock survives response loss; typed terminals drain before router replacement.
pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "state",
            from_fn(|_: UnlockContext| Ok::<_, Error>(ApiState::Locked)).no_cli(),
        )
        .subcommand("unlock", from_fn_async(unlock).no_cli())
}

impl UiContext for UnlockContext {
    fn ui_dir() -> &'static include_dir::Dir<'static> {
        UI_CELL.get().unwrap_or(&EMPTY_DIR)
    }
    fn api() -> ParentHandler<Self> {
        api()
    }
    fn middleware(server: Server<Self>) -> HttpServer<Self> {
        server.middleware(RequireJson)
    }
    fn apply_outer_layers(self, router: Router) -> Router {
        router.layer(axum::middleware::from_fn(
            move |request: Request<axum::body::Body>, next: axum::middleware::Next| {
                let context = self.clone();
                async move {
                    let name = crate::net::http::request_authority(&request)
                        .map(|authority| authority.host().to_owned());
                    let allowed =
                        crate::middleware::auth::signature::request_identities(&context, &request)
                            .await;
                    if name.is_some_and(|name| {
                        allowed
                            .is_ok_and(|names| names.iter().any(|candidate| &**candidate == name))
                    }) {
                        next.run(request).await
                    } else {
                        (
                            StatusCode::FORBIDDEN,
                            t!("net.static-server.unrecognized-host").into_owned(),
                        )
                            .into_response()
                    }
                }
            },
        ))
    }
}

#[tracing::instrument(skip_all)]
async fn unlock(
    rpc_toolkit::HandlerArgs {
        context,
        params,
        rpc_stream,
        ..
    }: rpc_toolkit::HandlerArgs<
        UnlockContext,
        UnlockParams,
        rpc_toolkit::Empty,
        rpc_toolkit::Empty,
    >,
) -> Result<(), Error> {
    let session = context.secure_session().ok_or_else(|| {
        Error::new(
            eyre!(t!("auth.encrypted-session-required")),
            ErrorKind::InvalidRequest,
        )
    })?;
    crate::middleware::auth::login_rate_limit::check(session.address().clone(), session.origin())?;
    let password = Zeroizing::new(params.password.decrypt_in_session(&context, &context)?);
    let streamed = rpc_stream.is_some();
    let operation = async move {
        let mut completion = context.completion.lock().await;
        let result = async {
            if completion.is_none() {
                return Ok(None);
            }
            let pool = LuksPool::new(&context.disk_guid, crate::disk::fde::HEADER_BACKUP_DIR)?;
            let memory = pool.protect_secret_handling()?;
            let (_, keys) = pool.capture_unlock_keys(&password).await?;
            drop(password);
            pool.cache_unlock_keys(&keys).await?;
            let result = import(&context.disk_guid, &keys, &context.progress).await?;
            memory.check()?;
            Ok::<_, Error>(Some(result))
        }
        .await;
        if let Some(stream) = rpc_stream {
            let _ = stream
                .close_and_flush(result.as_ref().map(|_| ()).map_err(Error::clone_output))
                .await;
        }
        if let Some(result) = result? {
            completion
                .take()
                .unwrap()
                .send(result)
                .map_err(|_| Error::new(eyre!(t!("error.cancelled")), ErrorKind::Cancelled))?;
        }
        Ok::<_, Error>(())
    };
    if streamed {
        tokio::spawn(async move {
            operation.await.log_err();
        });
        Ok(())
    } else {
        complete_owned_operation(operation).await
    }
}

async fn import(
    guid: &str,
    keys: &crate::disk::fde::VolumeKeys,
    progress: &FullProgressTracker,
) -> Result<RequiresReboot, Error> {
    #[cfg(target_os = "linux")]
    {
        let repair = if tokio::fs::try_exists(crate::disk::REPAIR_DISK_PATH).await? {
            RepairStrategy::Aggressive
        } else {
            RepairStrategy::Preen
        };
        let result = crate::disk::main::import_with_keys(
            guid,
            std::path::Path::new(DATA_DIR),
            keys,
            repair,
            Some(progress),
        )
        .await?;
        crate::util::io::delete_file(crate::disk::REPAIR_DISK_PATH).await?;
        Ok(result)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (guid, keys, progress);
        Err(invalid_handoff())
    }
}

pub(crate) async fn require_cached_unlock(guid: &str) -> Result<(), Error> {
    let pool = LuksPool::new(guid, crate::disk::fde::HEADER_BACKUP_DIR)?;
    let memory = pool.protect_secret_handling()?;
    if pool.cached_unlock_keys().await?.is_none() {
        return Err(invalid_handoff());
    }
    memory.check()
}

pub(crate) async fn initialize(
    server: &mut WebServer<WildcardListener>,
    dns: Arc<DnsController>,
    config: &ServerConfig,
) -> Result<Result<(RpcContext, FullProgressTracker), Shutdown>, Error> {
    let mut handoff = super::initramfs::load_handoff().await?;
    let guid = handoff.credentials.disk_guid.clone();
    let pool = LuksPool::new(&guid, crate::disk::fde::HEADER_BACKUP_DIR)?;
    let memory = pool.protect_secret_handling()?;
    crate::disk::main::activate_pool(&guid).await?;
    let init_context = InitContext::init(config, dns.clone()).await?;
    let handle = init_context.progress.clone();
    let init_phases = InitPhases::new(&handle);
    let rpc_phases = InitRpcContextPhases::new(&handle);
    let (reboot, acme) = match pool.cached_unlock_keys().await? {
        Some(keys) => (import(&guid, &keys, &handle).await?, None),
        None => {
            let mut bootstrap = UnlockNetwork::new(
                &mut handoff.credentials,
                dns.clone(),
                &server.acceptor_setter(),
                config,
            )
            .await?;
            let (context, mut result) =
                UnlockContext::new(bootstrap.host.clone(), guid.clone(), handle.clone());
            server.serve_ui_for(context.clone());
            let reboot = loop {
                tokio::select! {
                    result = &mut result => break result.with_kind(ErrorKind::Cancelled)?,
                    _ = bootstrap.changed() => bootstrap.reconcile().await?,
                }
            };
            bootstrap.retire().await?;
            (reboot, Some(bootstrap.acme.clone()))
        }
    };
    if reboot.0 {
        return Ok(Err(Shutdown {
            disk_guid: Some(guid),
            restart: true,
        }));
    }
    server.serve_ui_for(init_context.clone());
    let result = async {
        let context = crate::bins::start_init::initialize_data(
            server,
            dns,
            config,
            guid,
            init_phases,
            rpc_phases,
        )
        .await?;
        if let Some(acme) = acme {
            let snapshot: crate::net::acme::BootstrapAcme = acme.peek().await.de()?;
            context
                .db
                .mutate(move |db| snapshot.merge_cache_into(db))
                .await
                .result?;
        }
        handoff.credentials.remove_network_profiles().await?;
        memory.check()?;
        Ok::<_, Error>(context)
    }
    .await;
    init_context
        .complete_and_retire(result.as_ref().map(|_| ()).map_err(Error::clone_output))
        .await;
    Ok(Ok((result?, handle)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plaintext_unlock_rejects_before_accepting_disk_work() {
        let (context, mut completion) = UnlockContext::new(
            Watch::new(Model::new(&Host::new()).unwrap()),
            "STARTOS_NOT_ACTIVATED".into(),
            FullProgressTracker::new(),
        );
        let error = unlock(rpc_toolkit::HandlerArgs {
            context: context.clone(),
            params: UnlockParams {
                password: PasswordType::String("  secret  ".into()),
            },
            parent_method: Default::default(),
            method: Default::default(),
            inherited_params: rpc_toolkit::Empty {},
            raw_params: Value::Null,
            rpc_stream: None,
        })
        .await
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);
        assert!(context.completion.lock().await.is_some());
        assert!(matches!(
            completion.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
    }
}
