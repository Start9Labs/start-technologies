use super::*;
use crate::registry::context::call_legacy;
use crate::registry::package::get::GetPackagesResponseFull;
use crate::registry::package::index::{PackageIndex, PackageInfo};

/// Registries without WebSocket RPC refresh their catalog every fifteen minutes.
pub(super) const INTERVAL: Duration = Duration::from_secs(15 * 60);

pub(super) async fn poll(
    db: &TypedPatchDb<Database>,
    subscriptions: &BTreeSet<Subscription>,
    device: &DeviceInfo,
    client: &crate::context::rpc::ReloadableHttpClient,
    signer: &crate::sign::Signer,
) -> Result<(), Error> {
    for subscription in subscriptions {
        let registry = subscription.registry();
        let value = match subscription {
            Subscription::Index(_) => {
                let info = from_value(
                    call_legacy(
                        client,
                        signer,
                        registry,
                        http::HeaderMap::new(),
                        "info",
                        imbl_value::json!({}),
                    )
                    .await?,
                )?;
                let packages = from_value(
                    call_legacy(
                        client,
                        signer,
                        registry,
                        http::HeaderMap::new(),
                        "package.get",
                        imbl_value::json!({"otherVersions":"full", "allRevisions":true}),
                    )
                    .await?,
                )?;
                to_value(&index(info, packages))?
            }
            Subscription::Os(_) => {
                call_legacy(
                    client,
                    signer,
                    registry,
                    http::HeaderMap::new(),
                    "os.index",
                    imbl_value::json!({}),
                )
                .await?
            }
        };
        publish(db, subscription, &value, device).await?;
    }
    Ok(())
}

pub(super) fn index(info: RegistryInfo, packages: GetPackagesResponseFull) -> FullIndex {
    FullIndex {
        name: info.name,
        icon: info.icon,
        description: info.description,
        package: PackageIndex {
            categories: info.categories,
            packages: packages
                .into_iter()
                .map(|(id, package)| {
                    (
                        id,
                        PackageInfo {
                            categories: package.categories,
                            versions: package
                                .other_versions
                                .into_iter()
                                .chain(package.best)
                                .collect(),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        },
        trust_signers: info.trust_signers.keys().cloned().collect(),
        signers: info.trust_signers,
        ..Default::default()
    }
}
