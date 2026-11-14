//! Registries expose connecting/connected/error independently of cached catalog contents.
//! Connecting retains cached data; an empty connecting catalog is loading, not empty.
//! Connected includes successful polling without a persistent socket.
//! Retries clear failure details while retaining the last synchronized catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use patch_db::ModelExt;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::context::RpcContext;
use crate::db::model::public::ServerInfo;
use crate::db::model::{Database, DatabaseModel};
use crate::middleware::auth::signature::SigningContext;
use crate::prelude::*;
use crate::registry::FullIndex;
use crate::registry::device_info::DeviceInfo;
use crate::registry::info::RegistryInfo;
use crate::registry::os::index::{OsIndex, OsVersionInfo};
use crate::registry::package::get::{GetPackageParams, GetPackagesResponse, select_packages};
use crate::trusted_keys::connection::{
    connected_registry_urls, is_connected, refresh_registry, registry_url, same_registry,
};
use crate::util::future::NonDetachingJoinHandle;

pub(crate) mod connection;
mod polling;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum RegistryStatus {
    #[default]
    Connecting,
    Connected,
    Error,
}

rpc_toolkit::reflect_ts!(RegistryStatus);

/// Last synchronized package listings and registry trust proposals.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RegistryCache {
    pub info: RegistryInfo,
    pub packages: GetPackagesResponse,
    #[serde(default)]
    pub status: RegistryStatus,
    /// Failure of the latest synchronization attempt; absent while connecting or connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

rpc_toolkit::reflect_ts!(RegistryCache);

/// Compatible releases from the selected StartOS update source.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct OsUpdateCache {
    pub registry: Url,
    pub versions: BTreeMap<exver::Version, OsVersionInfo>,
    #[serde(default)]
    pub status: RegistryStatus,
    /// Failure of the latest synchronization attempt; absent while connecting or connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

rpc_toolkit::reflect_ts!(OsUpdateCache);

type ContextProvider = Arc<dyn Fn() -> Option<RpcContext> + Send + Sync>;
type RegistryTasks = BTreeMap<BTreeSet<Subscription>, NonDetachingJoinHandle<()>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Subscription {
    Index(Url),
    Os(Url),
}

impl Subscription {
    fn registry(&self) -> &Url {
        match self {
            Self::Index(url) | Self::Os(url) => url,
        }
    }

    fn pointer(&self) -> &'static str {
        match self {
            Self::Index(_) => "/index",
            Self::Os(_) => "/index/os",
        }
    }

    fn request_id(&self) -> u64 {
        match self {
            Self::Index(_) => 1,
            Self::Os(_) => 2,
        }
    }

    fn selected(&self, info: &ServerInfo) -> bool {
        match self {
            Self::Index(url) => is_connected(&info.registries, url.as_str()),
            Self::Os(url) => same_registry(info.os_registry.as_str(), url.as_str()),
        }
    }
}

fn subscriptions(info: &ServerInfo) -> BTreeSet<Subscription> {
    let indexes = connected_registry_urls(&info.registries).map(|url| url.map(Subscription::Index));
    let os = std::iter::once(registry_url(info.os_registry.as_str()).map(Subscription::Os));
    indexes
        .chain(os)
        .filter_map(|subscription| match subscription {
            Ok(subscription) => Some(subscription),
            Err(error) => {
                tracing::warn!(?error, "Skipping invalid saved registry source");
                None
            }
        })
        .collect()
}

fn connections(subscriptions: &BTreeSet<Subscription>) -> BTreeSet<BTreeSet<Subscription>> {
    let mut connections: BTreeMap<&Url, BTreeSet<Subscription>> = BTreeMap::new();
    for subscription in subscriptions {
        connections
            .entry(subscription.registry())
            .or_default()
            .insert(subscription.clone());
    }
    connections.into_values().collect()
}

async fn stop_registry_connections(tasks: &mut RegistryTasks, groups: Vec<BTreeSet<Subscription>>) {
    for group in groups {
        let _ = tasks.remove(&group).unwrap().wait_for_abort().await;
    }
}

async fn cancel_registry_connection(tasks: &mut RegistryTasks, registry: &Url) {
    let groups = tasks
        .keys()
        .filter(|subscriptions| {
            subscriptions
                .iter()
                .any(|subscription| subscription.registry() == registry)
        })
        .cloned()
        .collect();
    stop_registry_connections(tasks, groups).await;
}

/// Unrelated server-info writes do not wake catalog reconciliation.
struct RegistrySettingsWatch {
    registries: patch_db::DbWatch,
    os_registry: patch_db::DbWatch,
}

impl RegistrySettingsWatch {
    async fn new(db: &TypedPatchDb<Database>) -> Self {
        Self {
            registries: db
                .watch("/public/serverInfo/registries".parse().unwrap())
                .await,
            os_registry: db
                .watch("/public/serverInfo/osRegistry".parse().unwrap())
                .await,
        }
    }

    async fn read(&mut self, db: &TypedPatchDb<Database>) -> Result<ServerInfo, Error> {
        self.registries
            .peek_and_mark_seen()
            .with_kind(ErrorKind::Database)?;
        self.os_registry
            .peek_and_mark_seen()
            .with_kind(ErrorKind::Database)?;
        db.peek().await.as_public().as_server_info().de()
    }

    async fn changed(&mut self) -> Result<(), patch_db::Error> {
        tokio::select! {
            changed = self.registries.changed() => changed,
            changed = self.os_registry.changed() => changed,
        }
    }
}

pub async fn watch_registries(
    db: TypedPatchDb<Database>,
    mut refreshes: tokio::sync::mpsc::Receiver<Url>,
    context: impl Fn() -> Option<RpcContext> + Send + Sync + 'static,
) {
    let context: ContextProvider = Arc::new(context);
    let mut watch = RegistrySettingsWatch::new(&db).await;
    let mut tasks = RegistryTasks::new();
    loop {
        let desired = watch.read(&db).await.map(|info| subscriptions(&info));
        match desired {
            Ok(desired) => {
                let connections = connections(&desired);
                let removed = tasks
                    .keys()
                    .filter(|subscriptions| !connections.contains(*subscriptions))
                    .cloned()
                    .collect();
                stop_registry_connections(&mut tasks, removed).await;
                let result = db
                    .mutate(|db| {
                        reconcile(db, &desired, tasks.is_empty())?;
                        for subscriptions in connections
                            .iter()
                            .filter(|subscriptions| !tasks.contains_key(*subscriptions))
                        {
                            for subscription in subscriptions {
                                set_status(db, subscription, RegistryStatus::Connecting, None)?;
                            }
                        }
                        Ok::<_, Error>(())
                    })
                    .await
                    .result;
                if let Err(error) = result {
                    tracing::error!(?error, "Could not reconcile registry caches");
                    return;
                }
                for subscriptions in connections {
                    if !tasks.contains_key(&subscriptions) {
                        let db = db.clone();
                        let context = context.clone();
                        let key = subscriptions.clone();
                        tasks.insert(
                            key,
                            tokio::spawn(async move {
                                watch_registry(db, subscriptions, context).await;
                            })
                            .into(),
                        );
                    }
                }
            }
            Err(error) => tracing::warn!(?error, "Invalid registry configuration"),
        }
        tokio::select! {
            changed = watch.changed() => if changed.is_err() { return; },
            refresh = refreshes.recv() => match refresh {
                Some(registry) => cancel_registry_connection(&mut tasks, &registry).await,
                None => return,
            },
        }
    }
}

fn reconcile(
    db: &mut DatabaseModel,
    desired: &BTreeSet<Subscription>,
    reset: bool,
) -> Result<(), Error> {
    db.as_public_mut().mutate(|public| {
        public.registry_cache.retain(|url, cache| {
            if reset {
                cache.status = RegistryStatus::Connecting;
            }
            desired.contains(&Subscription::Index(url.clone()))
        });
        if public
            .os_update_cache
            .as_ref()
            .is_some_and(|cache| !desired.contains(&Subscription::Os(cache.registry.clone())))
        {
            public.os_update_cache = None;
        }
        if reset {
            if let Some(cache) = &mut public.os_update_cache {
                cache.status = RegistryStatus::Connecting;
            }
        }
        Ok(())
    })?;
    let mut indexes = db
        .as_private()
        .as_value()
        .get("registryIndexes")
        .map(|value| from_value::<BTreeMap<Url, FullIndex>>(value.clone()))
        .transpose()?
        .unwrap_or_default();
    indexes.retain(|url, _| desired.contains(&Subscription::Index(url.clone())));
    db.as_private_mut().as_registry_indexes_mut().ser(&indexes)
}

async fn watch_registry(
    db: TypedPatchDb<Database>,
    subscriptions: BTreeSet<Subscription>,
    context: ContextProvider,
) {
    let mut backoff = 30;
    loop {
        let _ = db
            .mutate(|db| {
                for subscription in &subscriptions {
                    set_status(db, subscription, RegistryStatus::Connecting, None)?;
                }
                Ok::<_, Error>(())
            })
            .await
            .result;
        let next_poll = tokio::time::Instant::now() + polling::INTERVAL;
        let result = connection::subscribe(&db, &subscriptions, &context, &mut backoff).await;
        let polling = result
            .as_ref()
            .is_err_and(connection::websocket_unsupported);
        let result = if polling {
            async {
                let Some(ctx) = context() else { return Ok(()) };
                let device = DeviceInfo::load(&ctx).await?;
                polling::poll(
                    &db,
                    &subscriptions,
                    &device,
                    &ctx.client,
                    &ctx.signing_key()?,
                )
                .await
            }
            .await
        } else {
            result
        };
        if let Err(error) = &result {
            tracing::debug!(?error, ?subscriptions, "Registry synchronization failed");
        }
        if !polling || result.is_err() {
            let error = result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_else(|| t!("registry.sync.closed").into_owned());
            let _ = db
                .mutate(|db| {
                    for subscription in &subscriptions {
                        set_status(db, subscription, RegistryStatus::Error, Some(&error))?;
                    }
                    Ok::<_, Error>(())
                })
                .await
                .result;
        }
        if context().is_none() {
            return;
        }
        let next_attempt = if polling {
            next_poll
        } else {
            let delay = Duration::from_secs(rand::random_range(backoff / 2..=backoff));
            backoff = (backoff * 2).min(300);
            tokio::time::Instant::now() + delay
        };
        tokio::time::sleep_until(next_attempt).await;
    }
}

fn set_status(
    db: &mut DatabaseModel,
    subscription: &Subscription,
    status: RegistryStatus,
    error: Option<&str>,
) -> Result<(), Error> {
    if !subscription.selected(&db.as_public().as_server_info().de()?) {
        return Ok(());
    }
    match subscription {
        Subscription::Index(url) => {
            db.as_public_mut()
                .as_registry_cache_mut()
                .mutate(|caches| {
                    let cache = caches.entry(url.clone()).or_insert_with(|| RegistryCache {
                        info: RegistryInfo::default(),
                        packages: BTreeMap::new(),
                        status,
                        error: None,
                    });
                    cache.status = status;
                    cache.error = error.map(str::to_owned);
                    Ok(())
                })?;
        }
        Subscription::Os(url) => {
            db.as_public_mut()
                .as_os_update_cache_mut()
                .mutate(|cache| {
                    if cache.as_ref().is_none_or(|cache| &cache.registry != url) {
                        *cache = Some(OsUpdateCache {
                            registry: url.clone(),
                            versions: BTreeMap::new(),
                            status,
                            error: error.map(str::to_owned),
                        });
                    } else if let Some(cache) = cache {
                        cache.status = status;
                        cache.error = error.map(str::to_owned);
                    }
                    Ok(())
                })?;
        }
    }
    Ok(())
}

async fn publish(
    db: &TypedPatchDb<Database>,
    subscription: &Subscription,
    value: &Value,
    device: &DeviceInfo,
) -> Result<(), Error> {
    db.mutate(|db| publish_snapshot(db, subscription, value, device))
        .await
        .result
}

fn publish_snapshot(
    db: &mut DatabaseModel,
    subscription: &Subscription,
    value: &Value,
    device: &DeviceInfo,
) -> Result<(), Error> {
    if !subscription.selected(&db.as_public().as_server_info().de()?) {
        return Ok(());
    }
    match subscription {
        Subscription::Index(url) => {
            let mut index: FullIndex = from_value(value.clone())?;
            index.os = OsIndex::default();
            let index = Model::from_value(to_value(&index)?);
            let info = crate::registry::info::from_index(&index)?;
            let mut package_device = device.clone();
            package_device.os.language = None;
            let packages = from_value(select_packages(
                index.as_package(),
                &GetPackageParams {
                    id: None,
                    target_version: None,
                    source_version: None,
                    device_info: Some(package_device),
                    other_versions: Some(crate::registry::package::get::PackageDetailLevel::Short),
                    all_revisions: false,
                    reader: Default::default(),
                },
            )?)?;
            refresh_registry(db, url, &info)?;
            db.as_private_mut()
                .as_registry_indexes_mut()
                .insert(url, &index.de()?)?;
            db.as_public_mut().as_registry_cache_mut().insert(
                url,
                &RegistryCache {
                    info,
                    packages,
                    status: RegistryStatus::Connected,
                    error: None,
                },
            )?;
        }
        Subscription::Os(url) => {
            let index: Model<OsIndex> = Model::from_value(value.clone());
            let versions = from_value(crate::registry::os::version::select_versions(
                &index,
                Some(&device.os.version),
                None,
                Some(&device.os.platform),
            )?)?;
            let name = db
                .as_public()
                .as_registry_cache()
                .as_idx(url)
                .map(|cache| cache.as_info().de().map(|info| info.name))
                .transpose()?
                .flatten();
            crate::update::security::notify_updates(db, url, name, index.de()?)?;
            db.as_public_mut()
                .as_os_update_cache_mut()
                .ser(&Some(OsUpdateCache {
                    registry: url.clone(),
                    versions,
                    status: RegistryStatus::Connected,
                    error: None,
                }))?;
        }
    }
    Ok(())
}

pub(crate) async fn cached_call(
    ctx: &RpcContext,
    registry: &Url,
    method: &str,
    params: &Value,
    device: Option<&DeviceInfo>,
) -> Result<Option<Value>, Error> {
    let url = registry_url(registry.as_str())?;
    cached_response(&ctx.db.peek().await, &url, method, params, device)
}

fn cached_response(
    snapshot: &DatabaseModel,
    url: &Url,
    method: &str,
    params: &Value,
    device: Option<&DeviceInfo>,
) -> Result<Option<Value>, Error> {
    if !is_connected(
        &snapshot.as_public().as_server_info().as_registries().de()?,
        url.as_str(),
    ) {
        return Ok(None);
    }
    match method {
        "info" => {
            let cache = snapshot
                .as_public()
                .as_registry_cache()
                .as_idx(url)
                .or_not_found("registry cache")?;
            Ok(Some(to_value(&cache.as_info().de()?)?))
        }
        "package.get" => {
            let mut params: GetPackageParams = from_value(params.clone())?;
            params.device_info = device.cloned();
            let empty = Model::new(&crate::registry::package::index::PackageIndex::default())?;
            let index = snapshot.as_private().as_registry_indexes().as_idx(url);
            Ok(Some(select_packages(
                index.map_or(&empty, |index| index.as_package()),
                &params,
            )?))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests;
