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
use crate::db::DbCursor;
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
    GetPackageParams, PackageSelection, select_packages, selection, update_target,
};
use crate::registry::package::index::PackageIndex;
use crate::registry::vulnerability::{OS_SUBJECT, Vulnerabilities};
use crate::s9pk::merkle_archive::source::encrypted::KeyStreamInfo;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::trusted_keys::connection::{
    connected_registry_urls, is_connected, refresh_registry, registry_url, same_registry,
};
use crate::trusted_keys::{TrustContext, enabled_keys};
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

/// A saved registry's synchronization state and what this server can run from its index.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RegistryCache {
    pub info: RegistryInfo,
    /// The versions of each package this server can run, in `registryIndexes`.
    #[serde(default)]
    pub selections: BTreeMap<PackageId, PackageSelection>,
    /// Whether the registry offers StartOS releases for this server's platform.
    #[serde(default)]
    pub publishes_os_updates: bool,
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
    /// Tracks the source registry's cached `RegistryInfo::default_locale`.
    #[serde(default)]
    pub default_locale: Option<InternedString>,
    #[serde(default)]
    pub status: RegistryStatus,
    /// Failure of the latest synchronization attempt; absent while connecting or connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// StartOS vulnerabilities the source publishes, signed or not by a trusted key.
    #[serde(default)]
    pub vulnerabilities: Vulnerabilities,
    /// Entries of `vulnerabilities` that a key trusted for StartOS updates signed.
    #[serde(default)]
    pub trusted_vulnerabilities: BTreeSet<InternedString>,
}

rpc_toolkit::reflect_ts!(OsUpdateCache);

/// What the registry projects an index subscription for.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryView {
    pub os_version: exver::Version,
    pub language: Option<InternedString>,
}

impl RegistryView {
    pub(crate) fn new(device: &DeviceInfo) -> Self {
        Self {
            os_version: device.os.version.clone(),
            language: device.os.language.clone(),
        }
    }
}

/// The saved index's last registry revision, within the view and epoch it was received in.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryCursor {
    /// Absent for registries that cannot resume.
    pub epoch: Option<InternedString>,
    pub id: u64,
    pub view: RegistryView,
}

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

async fn reconcile_connection_language(
    tasks: &mut RegistryTasks,
    previous: &mut Option<Option<InternedString>>,
    language: &Option<InternedString>,
) {
    if previous.as_ref() != Some(language) {
        let all = tasks.keys().cloned().collect();
        stop_registry_connections(tasks, all).await;
        *previous = Some(language.clone());
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
    language: patch_db::DbWatch,
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
            language: db
                .watch("/public/serverInfo/language".parse().unwrap())
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
        self.language
            .peek_and_mark_seen()
            .with_kind(ErrorKind::Database)?;
        db.peek().await.as_public().as_server_info().de()
    }

    async fn changed(&mut self) -> Result<(), patch_db::Error> {
        tokio::select! {
            changed = self.registries.changed() => changed,
            changed = self.os_registry.changed() => changed,
            changed = self.language.changed() => changed,
        }
    }
}

/// Every registry connection follows the saved server language.
pub async fn watch_registries(
    db: TypedPatchDb<Database>,
    mut refreshes: tokio::sync::mpsc::Receiver<Url>,
    context: impl Fn() -> Option<RpcContext> + Send + Sync + 'static,
) {
    let context: ContextProvider = Arc::new(context);
    let _update_targets: NonDetachingJoinHandle<()> =
        tokio::spawn(watch_installed_versions(db.clone(), context.clone())).into();
    let _vulnerability_trust: NonDetachingJoinHandle<()> =
        tokio::spawn(watch_trusted_keys(db.clone(), context.clone())).into();
    let mut watch = RegistrySettingsWatch::new(&db).await;
    let mut tasks = RegistryTasks::new();
    let mut language = None;
    loop {
        match watch.read(&db).await {
            Ok(info) => {
                reconcile_connection_language(&mut tasks, &mut language, &info.language).await;
                let desired = subscriptions(&info);
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
    let caches = db.as_public_mut().as_registry_cache_mut();
    for url in caches.keys()? {
        if !desired.contains(&Subscription::Index(url.clone())) {
            caches.remove(&url)?;
            continue;
        }
        let cache = caches.as_idx_mut(&url).or_not_found(&url)?;
        if let Some(fields) = cache.as_value_mut().as_object_mut() {
            for legacy in ["catalog", "packages", "listings"] {
                fields.remove(legacy);
            }
            if !fields.get("selections").is_some_and(Value::is_object) {
                fields.insert("selections".into(), Value::Object(Default::default()));
            }
        }
        if reset {
            cache.as_status_mut().ser(&RegistryStatus::Connecting)?;
        }
    }
    let os = db.as_public_mut().as_os_update_cache_mut();
    let selected = os
        .transpose_ref()
        .map(|cache| cache.as_registry().de())
        .transpose()?
        .map(|registry| desired.contains(&Subscription::Os(registry)));
    match selected {
        Some(false) => os.ser(&None)?,
        Some(true) if reset => {
            if let Some(cache) = os.transpose_mut() {
                cache.as_status_mut().ser(&RegistryStatus::Connecting)?;
            }
        }
        _ => (),
    }
    let stale = |url: &Url| !desired.contains(&Subscription::Index(url.clone()));
    if let Some(private) = db.as_private_mut().as_value_mut().as_object_mut() {
        private.remove("registryIndexes");
    }
    if db.as_public().as_value().get("registryIndexes").is_some() {
        for url in db.as_public().as_registry_indexes().keys()? {
            if stale(&url) {
                db.as_public_mut().as_registry_indexes_mut().remove(&url)?;
            }
        }
    } else {
        db.as_public_mut()
            .as_registry_indexes_mut()
            .ser(&BTreeMap::new())?;
    }
    let cursors = registry_cursors_mut(db)?;
    for url in cursors.keys()? {
        if stale(&url) {
            cursors.remove(&url)?;
        }
    }
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
        Subscription::Index(url) => set_index_status(cache_entry_mut(db, url)?, status, error)?,
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
                            vulnerabilities: BTreeMap::new(),
                            trusted_vulnerabilities: BTreeSet::new(),
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

/// The registry's cache entry, created empty and with a `selections` map when missing.
fn cache_entry_mut<'a>(
    db: &'a mut DatabaseModel,
    url: &Url,
) -> Result<&'a mut Model<RegistryCache>, Error> {
    let cache = db.as_public_mut().as_registry_cache_mut().upsert(url, || {
        Ok(RegistryCache {
            info: RegistryInfo::default(),
            selections: BTreeMap::new(),
            publishes_os_updates: false,
            status: RegistryStatus::Connecting,
            error: None,
        })
    })?;
    if !cache.as_selections().as_value().is_object() {
        cache.as_selections_mut().ser(&BTreeMap::new())?;
    }
    Ok(cache)
}

fn set_index_status(
    cache: &mut Model<RegistryCache>,
    status: RegistryStatus,
    error: Option<&str>,
) -> Result<(), Error> {
    cache.as_status_mut().ser(&status)?;
    if let Some(fields) = cache.as_value_mut().as_object_mut() {
        match error {
            Some(error) => fields.insert("error".into(), error.into()),
            None => fields.remove("error"),
        };
    }
    Ok(())
}

/// Rewrites the named packages' selections from the saved index, in place.
fn refresh_selections(
    db: &mut DatabaseModel,
    url: &Url,
    ids: BTreeSet<PackageId>,
    device: &DeviceInfo,
) -> Result<(), Error> {
    let Some(index) = db
        .as_public()
        .as_registry_indexes()
        .as_idx(url)
        .map(|index| index.as_package().clone())
    else {
        return Ok(());
    };
    let installed = installed_versions(db)?;
    let mut device = device.clone();
    device.os.language = None;
    let selections = ids
        .into_iter()
        .map(|id| {
            let target = installed
                .get(&id)
                .map(|version| update_target(&index, &id, version, &device))
                .transpose()?
                .flatten();
            Ok((selection(&index, &id, &device, target.as_ref())?, id))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let keys = enabled_keys(
        &db.as_public().as_server_info().as_trusted_keys().de()?,
        TrustContext::Marketplace,
    );
    let mut found = Vec::new();
    let packages = cache_entry_mut(db, url)?.as_selections_mut();
    for (mut selected, id) in selections {
        let published: Vulnerabilities = index
            .as_packages()
            .as_idx(&id)
            .map(|package| crate::registry::vulnerability::published(package.as_vulnerabilities()))
            .transpose()?
            .unwrap_or_default();
        let trusted = crate::registry::vulnerability::trusted(&id, &published, &keys);
        if selected.is_none() && !trusted.is_empty() {
            selected = Some(PackageSelection::default());
        }
        let Some(selected) = &mut selected else {
            packages.remove(&id)?;
            continue;
        };
        selected.vulnerabilities = trusted.keys().cloned().collect();
        let previous: BTreeSet<InternedString> = packages
            .as_idx(&id)
            .map(|previous| {
                from_value::<Option<_>>(previous.as_vulnerabilities().as_value().clone())
            })
            .transpose()?
            .flatten()
            .unwrap_or_default();
        if let Some(version) = installed.get(&id) {
            found.extend(
                trusted
                    .into_iter()
                    .filter(|(vid, v)| !previous.contains(vid) && v.affects(version))
                    .map(|(vid, v)| (id.clone(), version.clone(), vid, v)),
            );
        }
        packages.insert(&id, selected)?;
    }
    for (id, version, vid, vulnerability) in found {
        let title = db
            .as_public()
            .as_package_data()
            .as_idx(&id)
            .map(|entry| {
                entry
                    .as_state_info()
                    .as_manifest(ManifestPreference::Old)
                    .as_metadata()
                    .as_title()
                    .de()
            })
            .transpose()?
            .map_or_else(|| id.to_string(), |title| title.to_string());
        crate::registry::vulnerability::notify_found(
            db,
            url,
            Some((&id, &title)),
            &version,
            &vid,
            &vulnerability,
        )?;
    }
    Ok(())
}

/// Recomputes trusted StartOS vulnerabilities and notifies about new ones affecting this StartOS.
fn refresh_os_vulnerabilities(
    db: &mut DatabaseModel,
    previous: &BTreeSet<InternedString>,
    os_version: &exver::Version,
) -> Result<(), Error> {
    let Some(mut cache) = db.as_public().as_os_update_cache().de()? else {
        return Ok(());
    };
    let keys = enabled_keys(
        &db.as_public().as_server_info().as_trusted_keys().de()?,
        TrustContext::Os,
    );
    let trusted =
        crate::registry::vulnerability::trusted(OS_SUBJECT, &cache.vulnerabilities, &keys);
    let version = VersionString::from(exver::ExtendedVersion::new(
        os_version.clone(),
        exver::Version::default(),
    ));
    cache.trusted_vulnerabilities = trusted.keys().cloned().collect();
    let registry = cache.registry.clone();
    db.as_public_mut()
        .as_os_update_cache_mut()
        .ser(&Some(cache))?;
    for (vid, vulnerability) in trusted {
        if !previous.contains(&vid) && vulnerability.affects(&version) {
            crate::registry::vulnerability::notify_found(
                db,
                &registry,
                None,
                &version,
                &vid,
                &vulnerability,
            )?;
        }
    }
    Ok(())
}

/// Reapplies vulnerability trust after the server's trusted keys change.
fn refresh_trusted_vulnerabilities(
    db: &mut DatabaseModel,
    device: &DeviceInfo,
) -> Result<(), Error> {
    let mut registries = Vec::new();
    for (url, index) in db.as_public().as_registry_indexes().as_entries()? {
        let ids: BTreeSet<PackageId> = index
            .as_package()
            .as_packages()
            .keys()?
            .into_iter()
            .chain(
                db.as_public()
                    .as_registry_cache()
                    .as_idx(&url)
                    .map(|cache| cache.as_selections().keys())
                    .transpose()?
                    .unwrap_or_default(),
            )
            .collect();
        registries.push((url, ids));
    }
    for (url, ids) in registries {
        refresh_selections(db, &url, ids, device)?;
    }
    let previous = db
        .as_public()
        .as_os_update_cache()
        .de()?
        .map(|cache| cache.trusted_vulnerabilities)
        .unwrap_or_default();
    refresh_os_vulnerabilities(db, &previous, &device.os.version)
}

async fn watch_trusted_keys(db: TypedPatchDb<Database>, context: ContextProvider) {
    let mut watch = db
        .watch("/public/serverInfo/trustedKeys".parse().unwrap())
        .await;
    loop {
        if watch.peek_and_mark_seen().is_err() || watch.changed().await.is_err() {
            return;
        }
        let result = async {
            let Some(ctx) = context() else {
                return Ok(());
            };
            let device = DeviceInfo::load(&ctx).await?;
            db.mutate(|db| refresh_trusted_vulnerabilities(db, &device))
                .await
                .result
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(?error, "Could not reapply vulnerability trust");
        }
    }
}

fn refresh_update_targets(db: &mut DatabaseModel, device: &DeviceInfo) -> Result<(), Error> {
    let installed = installed_versions(db)?;
    for url in db.as_public().as_registry_indexes().keys()? {
        let mut ids: BTreeSet<PackageId> = installed.keys().cloned().collect();
        if let Some(cache) = db.as_public().as_registry_cache().as_idx(&url) {
            for (id, selected) in cache.as_selections().as_entries()? {
                if selected.as_value().get("updateTarget").is_some() {
                    ids.insert(id);
                }
            }
        }
        refresh_selections(db, &url, ids, device)?;
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
            let device = DeviceInfo::load(&ctx).await?;
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

/// The cursor to resume an index subscription from, when the saved index came from this view.
fn resumable_cursor(
    db: &DatabaseModel,
    url: &Url,
    view: &RegistryView,
) -> Result<Option<DbCursor>, Error> {
    if db.as_public().as_registry_indexes().as_idx(url).is_none() {
        return Ok(None);
    }
    Ok(db
        .as_private()
        .as_registry_cursors()
        .as_idx(url)
        .map(|cursor| cursor.de())
        .transpose()?
        .filter(|cursor| &cursor.view == view)
        .and_then(|cursor| {
            cursor.epoch.map(|epoch| DbCursor {
                epoch,
                id: cursor.id,
            })
        }))
}

/// A failed event forgets the cursor, so the next connection starts from a dump.
async fn ingest(
    db: &TypedPatchDb<Database>,
    url: &Url,
    view: &RegistryView,
    event: &connection::SubscriptionEvent,
    device: &DeviceInfo,
) -> Result<(), Error> {
    let result = db
        .mutate(|db| ingest_event(db, url, view, event, device))
        .await
        .result;
    if result.is_err() {
        let _ = db.mutate(|db| forget_cursor(db, url)).await.result;
    }
    result
}

fn registry_cursors_mut(
    db: &mut DatabaseModel,
) -> Result<&mut Model<BTreeMap<Url, RegistryCursor>>, Error> {
    let cursors = db.as_private_mut().as_registry_cursors_mut();
    if !matches!(cursors.as_value(), Value::Object(_)) {
        cursors.ser(&BTreeMap::new())?;
    }
    Ok(cursors)
}

fn forget_cursor(db: &mut DatabaseModel, url: &Url) -> Result<(), Error> {
    registry_cursors_mut(db)?.remove(url)?;
    Ok(())
}

fn saved_cursor(db: &DatabaseModel, url: &Url) -> Result<RegistryCursor, Error> {
    db.as_private()
        .as_registry_cursors()
        .as_idx(url)
        .or_not_found(lazy_format!("registry cursor for {url}"))?
        .de()
}

fn ingest_event(
    db: &mut DatabaseModel,
    url: &Url,
    view: &RegistryView,
    event: &connection::SubscriptionEvent,
    device: &DeviceInfo,
) -> Result<(), Error> {
    use connection::SubscriptionEvent;
    if !Subscription::Index(url.clone()).selected(&db.as_public().as_server_info().de()?) {
        return Ok(());
    }
    let (epoch, id, changed) = match event {
        SubscriptionEvent::Start(start) => match &start.revisions {
            None => {
                from_value::<FullIndex>(start.value.clone())?;
                save_index(db, url, start.value.clone())?;
                (start.epoch.clone(), start.id, None)
            }
            Some(revisions) => {
                let cursor = saved_cursor(db, url)?;
                ensure_code!(
                    cursor.epoch.is_some() && cursor.epoch == start.epoch && &cursor.view == view,
                    ErrorKind::Network,
                    "{}",
                    t!("registry.sync.unrequested-replay")
                );
                let changed = apply_revisions(db, url, cursor.id, revisions)?;
                (start.epoch.clone(), start.id, changed)
            }
        },
        SubscriptionEvent::Revision(revision) => {
            let cursor = saved_cursor(db, url)?;
            let changed = apply_revisions(db, url, cursor.id, std::slice::from_ref(revision))?;
            (cursor.epoch, revision.id, changed)
        }
    };
    registry_cursors_mut(db)?.insert(
        url,
        &RegistryCursor {
            epoch,
            id,
            view: view.clone(),
        },
    )?;
    refresh_public(db, url, changed.as_ref(), device)?;
    sync_os_default_locale(db)
}

fn save_index(db: &mut DatabaseModel, url: &Url, index: Value) -> Result<(), Error> {
    db.as_public_mut()
        .as_registry_indexes_mut()
        .insert_model(url, Model::from_value(index))?;
    Ok(())
}

/// Applies revisions to the saved index; `None` when a revision may change every package.
fn apply_revisions(
    db: &mut DatabaseModel,
    url: &Url,
    mut last: u64,
    revisions: &[patch_db::Revision],
) -> Result<Option<BTreeSet<PackageId>>, Error> {
    let index = db
        .as_public_mut()
        .as_registry_indexes_mut()
        .as_idx_mut(url)
        .or_not_found(lazy_format!("registry index for {url}"))?;
    let mut changed = Some(BTreeSet::new());
    for revision in revisions {
        ensure_code!(
            revision.id > last,
            ErrorKind::Database,
            "out-of-order registry revision"
        );
        patch_db::json_patch::patch(index.as_value_mut(), &revision.patch)
            .with_kind(ErrorKind::Database)?;
        changed = changed
            .zip(changed_packages(&revision.patch))
            .map(|(mut all, ids)| {
                all.extend(ids);
                all
            });
        last = revision.id;
    }
    Ok(changed)
}

/// Packages whose selection a patch to an index may change; `None` for all of them.
/// StartOS releases change no package.
pub(crate) fn changed_packages(patch: &patch_db::DiffPatch) -> Option<BTreeSet<PackageId>> {
    let mut changed = BTreeSet::new();
    for path in patch.paths() {
        match (
            path.get_segment(0),
            path.get_segment(1),
            path.get_segment(2),
        ) {
            (Some("os"), _, _) => (),
            (Some("package"), Some("packages"), Some(id)) => {
                changed.insert(id.parse().ok()?);
            }
            _ => return None,
        }
    }
    Some(changed)
}

/// Updates a registry's public cache entry in place from its saved index.
/// `changed` names the packages whose selections may differ, `None` meaning all of them;
/// registry-level fields are always refreshed.
pub(crate) fn refresh_public(
    db: &mut DatabaseModel,
    url: &Url,
    changed: Option<&BTreeSet<PackageId>>,
    device: &DeviceInfo,
) -> Result<(), Error> {
    let index = db
        .as_public()
        .as_registry_indexes()
        .as_idx(url)
        .or_not_found(lazy_format!("registry index for {url}"))?
        .clone();
    let info = crate::registry::info::info_value(&index)?;
    let publishes_os_updates = crate::registry::os::version::select_versions(
        index.as_os(),
        None,
        None,
        Some(&device.os.platform),
    )?
    .as_object()
    .is_some_and(|versions| !versions.is_empty());
    let cache = cache_entry_mut(db, url)?;
    let selections = cache.as_selections().as_value().as_object();
    let full = changed.is_none() || selections.is_none_or(|selections| selections.is_empty());
    let trust_changed =
        full || cache.as_info().as_value().get("trustSigners") != info.get("trustSigners");
    let ids = match changed.filter(|_| !full) {
        Some(changed) => changed.clone(),
        None => index
            .as_package()
            .as_packages()
            .keys()?
            .into_iter()
            .chain(cache.as_selections().keys()?)
            .collect(),
    };
    *cache.as_info_mut().as_value_mut() = info.clone();
    cache
        .as_publishes_os_updates_mut()
        .ser(&publishes_os_updates)?;
    set_index_status(cache, RegistryStatus::Connected, None)?;
    refresh_selections(db, url, ids, device)?;
    if trust_changed {
        refresh_registry(db, url, &from_value(info)?)?;
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
            let index: FullIndex = from_value(value.clone())?;
            save_index(db, url, to_value(&index)?)?;
            forget_cursor(db, url)?;
            refresh_public(db, url, None, device)?;
        }
        Subscription::Os(url) => {
            let index: Model<OsIndex> = Model::from_value(value.clone());
            let versions = from_value(crate::registry::os::version::select_versions(
                &index,
                Some(&device.os.version),
                None,
                Some(&device.os.platform),
            )?)?;
            let previous = db
                .as_public()
                .as_os_update_cache()
                .de()?
                .filter(|cache| same_registry(cache.registry.as_str(), url.as_str()))
                .map(|cache| cache.trusted_vulnerabilities)
                .unwrap_or_default();
            let vulnerabilities =
                crate::registry::vulnerability::published(index.as_vulnerabilities())?;
            db.as_public_mut()
                .as_os_update_cache_mut()
                .ser(&Some(OsUpdateCache {
                    registry: url.clone(),
                    versions,
                    default_locale: None,
                    status: RegistryStatus::Connected,
                    error: None,
                    vulnerabilities,
                    trusted_vulnerabilities: BTreeSet::new(),
                }))?;
            refresh_os_vulnerabilities(db, &previous, &device.os.version)?;
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

/// The keystream of a synchronized s9pk hosted at the URL.
pub(crate) fn synced_keystream(
    db: &DatabaseModel,
    url: &Url,
    commitment: &MerkleArchiveCommitment,
) -> Result<Option<KeyStreamInfo>, Error> {
    for (_, index) in db.as_public().as_registry_indexes().as_entries()? {
        if let Some(asset) = index
            .as_package()
            .s9pks_with(commitment)?
            .into_iter()
            .find(|asset| asset.urls.contains(url))
        {
            return Ok(asset.private);
        }
    }
    Ok(None)
}

fn synced_package_index<'a>(
    db: &'a DatabaseModel,
    registry: &Url,
) -> Result<Option<&'a Model<PackageIndex>>, Error> {
    let url = registry_url(registry.as_str())?;
    if !is_connected(
        &db.as_public().as_server_info().as_registries().de()?,
        url.as_str(),
    ) {
        return Err(not_saved(registry));
    }
    match db.as_public().as_registry_indexes().as_idx(&url) {
        Some(index) => Ok(Some(index.as_package())),
        None => {
            if let Some(cache) = db.as_public().as_registry_cache().as_idx(&url) {
                if cache.as_status().de()? == RegistryStatus::Error {
                    return Err(unreachable(registry, cache.as_error().de()?.as_deref()));
                }
            }
            Ok(None)
        }
    }
}

/// Selects packages from a saved registry's synchronized index.
/// A registry with no index yet selects nothing unless its synchronization failed.
pub(crate) fn synced_packages(
    db: &DatabaseModel,
    registry: &Url,
    params: &GetPackageParams,
) -> Result<Value, Error> {
    let empty = Model::new(&PackageIndex::default())?;
    select_packages(
        synced_package_index(db, registry)?.unwrap_or(&empty),
        params,
    )
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
