use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use patch_db::ModelExt;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::context::RpcContext;
use crate::db::model::{Database, DatabaseModel};
use crate::prelude::*;
use crate::registry::FullIndex;
use crate::registry::device_info::DeviceInfo;
use crate::registry::info::RegistryInfo;
use crate::registry::os::index::{OsIndex, OsVersionInfo};
use crate::registry::package::get::{GetPackageParams, GetPackagesResponse, select_packages};
use crate::trusted_keys::connection::{
    is_connected, refresh_registry, registry_url, same_registry,
};
use crate::util::future::NonDetachingJoinHandle;

mod connection;

/// Last synchronized package listings and registry trust proposals.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RegistryCache {
    pub info: RegistryInfo,
    pub packages: GetPackagesResponse,
    pub connected: bool,
}

rpc_toolkit::reflect_ts!(RegistryCache);

/// Compatible releases from the selected StartOS update source.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct OsUpdateCache {
    pub registry: Url,
    pub versions: BTreeMap<exver::Version, OsVersionInfo>,
    pub connected: bool,
}

rpc_toolkit::reflect_ts!(OsUpdateCache);

type ContextProvider = Arc<dyn Fn() -> Option<RpcContext> + Send + Sync>;

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

    fn selected(&self, ui: &Value) -> bool {
        match self {
            Self::Index(url) => is_connected(ui, url.as_str()),
            Self::Os(url) => ui
                .get("startosRegistry")
                .and_then(Value::as_str)
                .is_some_and(|selected| same_registry(selected, url.as_str())),
        }
    }
}

fn subscriptions(ui: &Value) -> Result<BTreeSet<Subscription>, Error> {
    let mut subscriptions = BTreeSet::new();
    if let Some(registries) = ui.get("registries").and_then(Value::as_object) {
        for url in registries.keys() {
            subscriptions.insert(Subscription::Index(registry_url(url)?));
        }
    }
    if let Some(url) = ui.get("startosRegistry").and_then(Value::as_str) {
        subscriptions.insert(Subscription::Os(registry_url(url)?));
    }
    Ok(subscriptions)
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

pub async fn watch_registries(
    db: TypedPatchDb<Database>,
    context: impl Fn() -> Option<RpcContext> + Send + Sync + 'static,
) {
    let context: ContextProvider = Arc::new(context);
    let mut watch = db.watch("/public/ui".parse().unwrap()).await;
    let mut tasks: BTreeMap<BTreeSet<Subscription>, NonDetachingJoinHandle<()>> = BTreeMap::new();
    loop {
        let desired = watch
            .peek_and_mark_seen()
            .with_kind(ErrorKind::Database)
            .and_then(|ui| subscriptions(&ui));
        match desired {
            Ok(desired) => {
                let connections = connections(&desired);
                tasks.retain(|subscriptions, _| connections.contains(subscriptions));
                let result = db
                    .mutate(|db| {
                        reconcile(db, &desired, tasks.is_empty())?;
                        for subscriptions in connections
                            .iter()
                            .filter(|subscriptions| !tasks.contains_key(*subscriptions))
                        {
                            for subscription in subscriptions {
                                set_disconnected(db, subscription)?;
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
        if watch.changed().await.is_err() {
            return;
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
                cache.connected = false;
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
                cache.connected = false;
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
        let result = connection::subscribe(&db, &subscriptions, &context, &mut backoff).await;
        if let Err(error) = result {
            tracing::debug!(?error, ?subscriptions, "Registry connection disconnected");
        }
        let _ = db
            .mutate(|db| {
                for subscription in &subscriptions {
                    set_disconnected(db, subscription)?;
                }
                Ok::<_, Error>(())
            })
            .await
            .result;
        if context().is_none() {
            return;
        }
        let delay = Duration::from_secs(rand::random_range(backoff / 2..=backoff));
        backoff = (backoff * 2).min(300);
        tokio::time::sleep(delay).await;
    }
}

fn set_disconnected(db: &mut DatabaseModel, subscription: &Subscription) -> Result<(), Error> {
    if !subscription.selected(&db.as_public().as_ui().de()?) {
        return Ok(());
    }
    match subscription {
        Subscription::Index(url) => {
            db.as_public_mut()
                .as_registry_cache_mut()
                .mutate(|caches| {
                    caches
                        .entry(url.clone())
                        .or_insert_with(|| RegistryCache {
                            info: RegistryInfo::default(),
                            packages: BTreeMap::new(),
                            connected: false,
                        })
                        .connected = false;
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
                            connected: false,
                        });
                    } else if let Some(cache) = cache {
                        cache.connected = false;
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
    if !subscription.selected(&db.as_public().as_ui().de()?) {
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
                    connected: true,
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
                    connected: true,
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
    if !is_connected(&snapshot.as_public().as_ui().de()?, url.as_str()) {
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
