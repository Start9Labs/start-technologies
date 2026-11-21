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

use crate::PackageId;
use crate::context::RpcContext;
use crate::db::model::package::{ManifestPreference, PackageStateMatchModelRef};
use crate::db::model::public::ServerInfo;
use crate::db::model::{Database, DatabaseModel};
use crate::middleware::auth::signature::SigningContext;
use crate::prelude::*;
use crate::registry::FullIndex;
use crate::registry::device_info::DeviceInfo;
use crate::registry::info::RegistryInfo;
use crate::registry::os::index::{OsIndex, OsVersionInfo};
use crate::registry::package::get::{
    GetPackageParams, GetPackageResponse, PackageDetailLevel, PackageReleases, select_packages,
    select_releases,
};
use crate::registry::package::index::PackageIndex;
use crate::trusted_keys::connection::{
    connected_registry_urls, is_connected, refresh_registry, registry_url, same_registry,
};
use crate::util::VersionString;
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
    /// Releases this server can run, read by the UI in place of `package.get`.
    #[serde(default)]
    pub catalog: Catalog,
    /// Whether the registry offers StartOS releases for this server's platform.
    #[serde(default)]
    pub publishes_os_updates: bool,
    #[serde(default)]
    pub status: RegistryStatus,
    /// Failure of the latest synchronization attempt; absent while connecting or connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub type Catalog = BTreeMap<PackageId, PackageReleases>;

rpc_toolkit::reflect_ts!(RegistryCache);

/// Compatible releases from the selected StartOS update source.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct OsUpdateCache {
    pub registry: Url,
    pub versions: BTreeMap<exver::Version, OsVersionInfo>,
    /// Tracks the source registry's cached `RegistryInfo::default_locale`.
    #[serde(default)]
    pub default_locale: Option<InternedString>,
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
    let _update_targets: NonDetachingJoinHandle<()> =
        tokio::spawn(watch_installed_versions(db.clone(), context.clone())).into();
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
    db.as_private_mut()
        .as_registry_indexes_mut()
        .ser(&indexes)?;
    sync_os_default_locale(db)
}

async fn watch_registry(
    db: TypedPatchDb<Database>,
    subscriptions: BTreeSet<Subscription>,
    context: ContextProvider,
) {
    let mut backoff = INITIAL_BACKOFF;
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
            tokio::time::Instant::now() + next_retry(&mut backoff)
        };
        tokio::time::sleep_until(next_attempt).await;
    }
}

/// Seconds before the first reconnection attempt after a successful update.
pub(crate) const INITIAL_BACKOFF: u64 = 30;

/// Returns a jittered reconnection delay and doubles the backoff, up to five minutes.
pub(crate) fn next_retry(backoff: &mut u64) -> Duration {
    let delay = Duration::from_secs(rand::random_range(*backoff / 2..=*backoff));
    *backoff = (*backoff * 2).min(300);
    delay
}

fn sync_os_default_locale(db: &mut DatabaseModel) -> Result<(), Error> {
    let public = db.as_public();
    let Some(cache) = public.as_os_update_cache().transpose_ref() else {
        return Ok(());
    };
    let registry = cache.as_registry().de()?;
    let default_locale = public
        .as_registry_cache()
        .as_idx(&registry)
        .map(|cache| cache.as_info().de().map(|info| info.default_locale))
        .transpose()?
        .flatten();
    db.as_public_mut()
        .as_os_update_cache_mut()
        .transpose_mut()
        .unwrap()
        .as_default_locale_mut()
        .ser(&default_locale)
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
                        catalog: BTreeMap::new(),
                        publishes_os_updates: false,
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
                            default_locale: None,
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
    sync_os_default_locale(db)
}

fn installed_versions(db: &DatabaseModel) -> Result<BTreeMap<PackageId, VersionString>, Error> {
    let mut installed = BTreeMap::new();
    for (id, entry) in db.as_public().as_package_data().as_entries()? {
        if matches!(
            entry.as_state_info().as_match(),
            PackageStateMatchModelRef::Installed(_) | PackageStateMatchModelRef::Updating(_)
        ) {
            let manifest = entry.as_state_info().as_manifest(ManifestPreference::Old);
            installed.insert(id, manifest.as_version().de()?);
        }
    }
    Ok(installed)
}

/// Reachability follows the target's `sourceVersion`, as in `package.get`.
fn update_targets(
    index: &Model<PackageIndex>,
    installed: &BTreeMap<PackageId, VersionString>,
    device: &DeviceInfo,
) -> Result<BTreeMap<PackageId, VersionString>, Error> {
    let mut targets = BTreeMap::new();
    for (id, version) in installed {
        let response: GetPackageResponse = from_value(select_packages(
            index,
            &GetPackageParams {
                id: Some(id.clone()),
                target_version: None,
                provides: None,
                source_version: Some(version.clone()),
                device_info: Some(device.clone()),
                other_versions: Some(PackageDetailLevel::None),
                all_revisions: false,
                reader: Default::default(),
            },
        )?)?;
        if let Some(target) = response.best.into_keys().find(|target| {
            target.flavor() == version.flavor()
                && target.partial_cmp(version) == Some(std::cmp::Ordering::Greater)
        }) {
            targets.insert(id.clone(), target);
        }
    }
    Ok(targets)
}

fn refresh_update_targets(db: &mut DatabaseModel, device: &DeviceInfo) -> Result<(), Error> {
    let installed = installed_versions(db)?;
    let targets = db
        .as_private()
        .as_registry_indexes()
        .as_entries()?
        .into_iter()
        .map(|(url, index)| Ok((url, update_targets(index.as_package(), &installed, device)?)))
        .collect::<Result<Vec<_>, Error>>()?;
    for (url, mut targets) in targets {
        if let Some(cache) = db.as_public_mut().as_registry_cache_mut().as_idx_mut(&url) {
            for (id, package) in cache.as_catalog_mut().as_entries_mut()? {
                package.as_update_target_mut().ser(&targets.remove(&id))?;
            }
        }
    }
    Ok(())
}

/// Recomputes update targets whenever an installed package's version changes.
async fn watch_installed_versions(db: TypedPatchDb<Database>, context: ContextProvider) {
    let mut watch = db.watch("/public/packageData".parse().unwrap()).await;
    let mut last = None;
    loop {
        if watch.peek_and_mark_seen().is_err() {
            return;
        }
        let result = async {
            let installed = installed_versions(&db.peek().await)?;
            if last.as_ref() == Some(&installed) {
                return Ok(());
            }
            let Some(ctx) = context() else {
                return Ok(());
            };
            let mut device = DeviceInfo::load(&ctx).await?;
            device.os.language = None;
            db.mutate(|db| refresh_update_targets(db, &device))
                .await
                .result?;
            last = Some(installed);
            Ok::<_, Error>(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(?error, "Could not refresh package update targets");
        }
        if watch.changed().await.is_err() {
            return;
        }
    }
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
            let publishes_os_updates = !from_value::<BTreeMap<exver::Version, OsVersionInfo>>(
                crate::registry::os::version::select_versions(
                    &Model::from_value(to_value(&index.os)?),
                    None,
                    None,
                    Some(&device.os.platform),
                )?,
            )?
            .is_empty();
            index.os = OsIndex::default();
            let index = Model::from_value(to_value(&index)?);
            let info = crate::registry::info::from_index(&index)?;
            let mut package_device = device.clone();
            package_device.os.language = None;
            let mut catalog = select_releases(index.as_package(), &package_device.for_listing())?;
            let mut targets = update_targets(
                index.as_package(),
                &installed_versions(db)?,
                &package_device,
            )?;
            for (id, package) in &mut catalog {
                package.update_target = targets.remove(id);
            }
            refresh_registry(db, url, &info)?;
            db.as_private_mut()
                .as_registry_indexes_mut()
                .insert(url, &index.de()?)?;
            db.as_public_mut().as_registry_cache_mut().insert(
                url,
                &RegistryCache {
                    info,
                    catalog,
                    publishes_os_updates,
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
                    default_locale: None,
                    status: RegistryStatus::Connected,
                    error: None,
                }))?;
        }
    }
    sync_os_default_locale(db)
}

fn not_saved(registry: &Url) -> Error {
    Error::new(
        eyre!(
            "{}",
            t!("registry.sync.not-saved", registry = registry.as_str())
        ),
        ErrorKind::NotFound,
    )
}

fn unreachable(registry: &Url, error: Option<&str>) -> Error {
    Error::new(
        eyre!(
            "{}",
            t!(
                "registry.sync.unreachable",
                registry = registry.as_str(),
                error = error.unwrap_or_default()
            )
        ),
        ErrorKind::Network,
    )
}

/// Selects packages from a saved registry's synchronized index.
/// A registry with no index yet selects nothing unless its synchronization failed.
pub(crate) fn synced_packages(
    db: &DatabaseModel,
    registry: &Url,
    params: &GetPackageParams,
) -> Result<Value, Error> {
    let url = registry_url(registry.as_str())?;
    if !is_connected(
        &db.as_public().as_server_info().as_registries().de()?,
        url.as_str(),
    ) {
        return Err(not_saved(registry));
    }
    let empty = Model::new(&PackageIndex::default())?;
    let index = match db.as_private().as_registry_indexes().as_idx(&url) {
        Some(index) => index.as_package(),
        None => {
            let cache = db
                .as_public()
                .as_registry_cache()
                .as_idx(&url)
                .map(|cache| cache.de())
                .transpose()?;
            if let Some(cache) = cache.filter(|cache| cache.status == RegistryStatus::Error) {
                return Err(unreachable(registry, cache.error.as_deref()));
            }
            &empty
        }
    };
    select_packages(index, params)
}

/// The selected StartOS update source's synchronized releases for this server.
/// Selection follows `serverInfo.osRegistry`, independently of cache reconciliation.
/// An empty cache whose synchronization failed is unreachable, not evidence of no updates.
pub(crate) fn synced_os_versions(
    db: &DatabaseModel,
) -> Result<BTreeMap<exver::Version, OsVersionInfo>, Error> {
    let selected = db.as_public().as_server_info().as_os_registry().de()?;
    let Some(cache) = db
        .as_public()
        .as_os_update_cache()
        .de()?
        .filter(|cache| same_registry(cache.registry.as_str(), selected.as_str()))
    else {
        return Ok(BTreeMap::new());
    };
    if cache.status == RegistryStatus::Error && cache.versions.is_empty() {
        return Err(unreachable(&selected, cache.error.as_deref()));
    }
    Ok(cache.versions)
}

#[cfg(test)]
mod tests;
