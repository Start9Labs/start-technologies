use clap::Parser;
use http::HeaderMap;
use serde::{Deserialize, Serialize};
use url::Url;

use super::*;
use crate::db::model::public::Public;
use crate::middleware::auth::signature::SigningContext;
use crate::registry::info::RegistryInfo;
use crate::sign::Signer;

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct TrustedRegistryParams {
    #[arg(help = "help.arg.registry-url")]
    pub url: String,
}

rpc_toolkit::reflect_ts!(TrustedRegistryParams);

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ConnectRegistryParams {
    #[arg(help = "help.arg.registry-url")]
    pub url: String,
    #[arg(long, help = "help.arg.registry-name")]
    pub name: Option<String>,
}

rpc_toolkit::reflect_ts!(ConnectRegistryParams);

pub(crate) fn registry_url(value: &str) -> Result<Url, Error> {
    let mut url = value.parse::<Url>().with_kind(ErrorKind::ParseUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.invalid-registry-url")),
            ErrorKind::InvalidRequest,
        ));
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}

pub(crate) fn same_registry(first: &str, second: &str) -> bool {
    match (registry_url(first), registry_url(second)) {
        (Ok(first), Ok(second)) => first == second,
        _ => false,
    }
}

const BUILTIN_REGISTRIES: [(&str, &str); 2] = [
    ("https://registry.start9.com/", "Start9 Registry"),
    (
        "https://community-registry.start9.com/",
        "Community Registry",
    ),
];

#[cfg(feature = "beta")]
const BETA_REGISTRIES: [(&str, &str); 3] = [
    ("https://beta-registry.start9.com/", "Start9 Beta Registry"),
    (
        "https://community-beta-registry.start9.com/",
        "Community Beta Registry",
    ),
    (
        "https://alpha-registry-x.start9.com/",
        "Start9 Alpha Registry",
    ),
];

#[cfg(not(feature = "beta"))]
const INITIAL_OS_REGISTRY: &str = "https://registry.start9.com/";
#[cfg(feature = "beta")]
const INITIAL_OS_REGISTRY: &str = "https://beta-registry.start9.com/";

/// Registries a new server starts connected to.
pub fn initial_registries() -> Registries {
    let mut registries = Registries::new();
    ensure_builtin_registries(&mut registries);
    #[cfg(feature = "beta")]
    for (url, name) in BETA_REGISTRIES {
        registries.insert(url.into(), Some(name.into()));
    }
    registries
}

/// Registry a new server, or the diagnostic UI, takes StartOS updates from.
pub fn initial_os_registry() -> Url {
    INITIAL_OS_REGISTRY.parse().unwrap()
}

pub(crate) fn ensure_builtin_registries(registries: &mut Registries) {
    for (url, name) in BUILTIN_REGISTRIES {
        if !registries.keys().any(|saved| same_registry(saved, url)) {
            registries.insert(url.into(), Some(name.into()));
        }
    }
}

/// Reads the `registries` map StartOS kept in `public.ui` before 0.4.2.1.
pub(crate) fn ui_registries(ui: &Value) -> Registries {
    ui.get("registries")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(url, name)| (url.clone(), name.as_str().map(str::to_owned)))
        .collect()
}

pub(crate) fn connected_registry_urls(
    registries: &Registries,
) -> impl Iterator<Item = Result<Url, Error>> + '_ {
    registries.keys().map(|url| registry_url(url))
}

pub(crate) fn is_connected(registries: &Registries, url: &str) -> bool {
    let Ok(url) = registry_url(url) else {
        return false;
    };
    connected_registry_urls(registries).any(|saved| saved.is_ok_and(|saved| saved == url))
}

fn offered_keys(public: &Public, url: &str) -> BTreeSet<AnyVerifyingKey> {
    public
        .server_info
        .registry_keys
        .iter()
        .filter(|(saved, _)| same_registry(saved, url))
        .flat_map(|(_, keys)| keys.iter().cloned())
        .collect()
}

async fn fetch_info(
    client: &crate::context::rpc::ReloadableHttpClient,
    url: &Url,
    signer: Signer,
) -> Result<RegistryInfo, Error> {
    let value = crate::registry::context::call(
        client,
        &signer,
        url,
        HeaderMap::new(),
        "info",
        imbl_value::json!({}),
    )
    .await?;
    from_value(value)
}

pub async fn list(ctx: RpcContext) -> Result<Registries, Error> {
    ctx.db
        .peek()
        .await
        .into_public()
        .into_server_info()
        .into_registries()
        .de()
}

/// Returns the keys it added, which are trusted for packages.
pub async fn connect(
    ctx: RpcContext,
    params: ConnectRegistryParams,
) -> Result<BTreeSet<InternedString>, Error> {
    let url = registry_url(&params.url)?;
    let info = fetch_info(&ctx.client, &url, ctx.signing_key()?).await?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| connect_public(public, &url, params.name, &info))
        })
        .await
        .result
}

/// Returns after queueing resubscription, not after receiving the refreshed catalog.
pub async fn refresh(ctx: RpcContext, params: TrustedRegistryParams) -> Result<(), Error> {
    let url = registry_url(&params.url)?;
    if !is_connected(
        &ctx.db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_registries()
            .de()?,
        url.as_str(),
    ) {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.registry-not-connected")),
            ErrorKind::InvalidRequest,
        ));
    }
    ctx.request_registry_refresh(url).await
}

pub(crate) fn refresh_registry(
    db: &mut crate::db::model::DatabaseModel,
    url: &Url,
    info: &RegistryInfo,
) -> Result<(), Error> {
    if !is_connected(
        &db.as_public().as_server_info().as_registries().de()?,
        url.as_str(),
    ) {
        return Ok(());
    }
    let registry = info
        .name
        .clone()
        .unwrap_or_else(|| url.host_str().unwrap_or_default().to_owned());
    let (added, dropped) = db.as_public_mut().mutate(|public| {
        let changes = refresh_public(public, &url, &info)?;
        let os_registry = same_registry(public.server_info.os_registry.as_str(), url.as_str());
        let keys = &public.server_info.trusted_keys;
        let name = |id: &InternedString| {
            keys.get(id)
                .map_or_else(|| id.to_string(), |key| key.name.clone())
        };
        let added = changes
            .added
            .into_iter()
            .map(|id| (name(&id), id))
            .collect::<Vec<_>>();
        let dropped = changes
            .dropped
            .into_iter()
            .map(|id| {
                let critical = os_registry || keys.get(&id).is_some_and(|key| key.os);
                (name(&id), critical, id)
            })
            .collect::<Vec<_>>();
        Ok((added, dropped))
    })?;
    for (name, key) in added {
        crate::notifications::notify(
            db,
            None,
            crate::notifications::NotificationLevel::Info,
            t!("trusted-keys.key-added-title").to_string(),
            t!(
                "trusted-keys.key-added-message",
                registry = registry.as_str(),
                name = name.as_str()
            )
            .to_string(),
            crate::notifications::RegistryKeyAdded {
                registry: url.to_string(),
                key,
            },
        )?;
    }
    for (name, critical, key) in dropped {
        crate::notifications::notify(
            db,
            None,
            if critical {
                crate::notifications::NotificationLevel::Warning
            } else {
                crate::notifications::NotificationLevel::Info
            },
            t!("trusted-keys.key-dropped-title").to_string(),
            t!(
                "trusted-keys.key-dropped-message",
                registry = registry.as_str(),
                name = name.as_str()
            )
            .to_string(),
            crate::notifications::RegistryKeyDropped {
                registry: url.to_string(),
                key,
            },
        )?;
    }
    Ok(())
}

/// Returns the registry-added keys the removal left unoffered.
pub async fn remove(
    ctx: RpcContext,
    params: TrustedRegistryParams,
) -> Result<BTreeSet<InternedString>, Error> {
    let url = registry_url(&params.url)?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| remove_public(public, &url))
        })
        .await
        .result
}

/// Returns the keys it newly trusted for OS updates.
pub async fn set_os_registry(
    ctx: RpcContext,
    params: TrustedRegistryParams,
) -> Result<BTreeSet<InternedString>, Error> {
    let url = registry_url(&params.url)?;
    crate::update::signer_files::mutate(ctx.db.clone(), move |db| {
        db.as_public_mut()
            .mutate(|public| set_os_registry_public(public, &url))
    })
    .await
}

fn forget_registry(
    registries: &mut Registries,
    snapshots: &mut BTreeMap<InternedString, BTreeSet<AnyVerifyingKey>>,
    url: &str,
) {
    registries.retain(|saved, _| !same_registry(saved, url));
    snapshots.retain(|saved, _| !same_registry(saved, url));
}

fn remove_public(public: &mut Public, url: &Url) -> Result<BTreeSet<InternedString>, Error> {
    if same_registry(public.server_info.os_registry.as_str(), url.as_str()) {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.os-registry-in-use")),
            ErrorKind::InvalidRequest,
        ));
    }
    let offered = offered_keys(public, url.as_str());
    let mut registries = public.server_info.registries.clone();
    let mut snapshots = public.server_info.registry_keys.clone();
    forget_registry(&mut registries, &mut snapshots, url.as_str());
    let mut keys = public.server_info.trusted_keys.clone();
    protect_registry_keys(&mut keys, &snapshots, &registries);
    let orphaned = keys
        .iter()
        .filter(|(_, key)| {
            key.removable && key.origin != MANUAL && offered.contains(&key.public_key)
        })
        .map(|(id, _)| id.clone())
        .collect();
    public.server_info.registries = registries;
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(orphaned)
}

fn set_os_registry_public(
    public: &mut Public,
    url: &Url,
) -> Result<BTreeSet<InternedString>, Error> {
    if !is_connected(&public.server_info.registries, url.as_str()) {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.registry-not-connected")),
            ErrorKind::NotFound,
        ));
    }
    let offered = offered_keys(public, url.as_str());
    let mut keys = public.server_info.trusted_keys.clone();
    let mut enabled = BTreeSet::new();
    for (id, key) in &mut keys {
        if !key.os && offered.contains(&key.public_key) {
            key.os = true;
            enabled.insert(id.clone());
        }
    }
    public.server_info.os_registry = url.clone();
    public.server_info.trusted_keys = keys;
    Ok(enabled)
}

fn proposed_keys(info: &RegistryInfo, origin: &str) -> Result<TrustedKeys, Error> {
    let now = Utc::now();
    let mut proposal = TrustedKeys::new();
    for signer in info.trust_signers.values() {
        let mut imported = split_import(
            signer.name.clone(),
            ImportedPublicKeys {
                keys: signer.keys.clone(),
                gpg_exports: signer.gpg_exports.clone(),
            },
            now,
        )?;
        for (id, key) in &mut imported {
            if let Some(name) = key.identity_name() {
                key.name = name;
            } else if key.name.trim().is_empty() {
                key.name = id.to_string();
            }
            key.origin = origin.into();
            key.origin_registry = Some(RegistryIdentity {
                name: info.name.clone(),
                icon: info.icon.clone(),
            });
            key.contact = signer.contact.iter().cloned().collect();
        }
        merge_individual_keys(&mut proposal, imported);
    }
    Ok(proposal)
}

fn connect_public(
    public: &mut Public,
    url: &Url,
    name: Option<String>,
    info: &RegistryInfo,
) -> Result<BTreeSet<InternedString>, Error> {
    let origin = url.as_str().trim_end_matches('/');
    let mut proposal = proposed_keys(info, origin)?;
    let proposed = proposal
        .values()
        .map(|key| key.public_key.clone())
        .collect();
    let mut keys = public.server_info.trusted_keys.clone();
    let mut added = BTreeSet::new();
    for (id, key) in &mut proposal {
        if !keys.contains_key(id) {
            key.marketplace = true;
            added.insert(id.clone());
        }
    }
    merge_individual_keys(&mut keys, proposal);
    let mut registries = public.server_info.registries.clone();
    let mut snapshots = public.server_info.registry_keys.clone();
    forget_registry(&mut registries, &mut snapshots, origin);
    registries.insert(origin.into(), name.or_else(|| info.name.clone()));
    snapshots.insert(origin.into(), proposed);
    protect_registry_keys(&mut keys, &snapshots, &registries);
    public.server_info.registries = registries;
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(added)
}

/// Keys new to the server, imported untrusted, and keys the registry stopped offering.
#[derive(Debug, Default, PartialEq, Eq)]
struct KeyChanges {
    added: BTreeSet<InternedString>,
    dropped: BTreeSet<InternedString>,
}

fn refresh_public(
    public: &mut Public,
    url: &Url,
    info: &RegistryInfo,
) -> Result<KeyChanges, Error> {
    let origin = url.as_str().trim_end_matches('/');
    if !is_connected(&public.server_info.registries, origin) {
        return Ok(KeyChanges::default());
    }
    let proposal = proposed_keys(info, origin)?;
    let proposed: BTreeSet<_> = proposal
        .values()
        .map(|key| key.public_key.clone())
        .collect();
    let dropped = offered_keys(public, origin)
        .difference(&proposed)
        .map(AnyVerifyingKey::interned_der64)
        .collect();
    let mut keys = public.server_info.trusted_keys.clone();
    let added = proposal
        .keys()
        .filter(|id| !keys.contains_key(*id))
        .cloned()
        .collect();
    let mut snapshots = public.server_info.registry_keys.clone();
    snapshots.retain(|saved, _| !same_registry(saved, origin));
    snapshots.insert(origin.into(), proposed);
    merge_individual_keys(&mut keys, proposal);
    protect_registry_keys(&mut keys, &snapshots, &public.server_info.registries);
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(KeyChanges { added, dropped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::package::promote::registry_rpc_url;

    #[test]
    fn cli_parses_registry_commands() {
        let add = ConnectRegistryParams::try_parse_from([
            "add",
            "https://registry.example",
            "--name",
            "Mine",
        ])
        .unwrap();
        assert_eq!(
            (add.url.as_str(), add.name.as_deref()),
            ("https://registry.example", Some("Mine"))
        );
        assert_eq!(
            ConnectRegistryParams::try_parse_from(["add", "https://registry.example"])
                .unwrap()
                .name,
            None
        );
        assert_eq!(
            TrustedRegistryParams::try_parse_from(["remove", "https://registry.example"])
                .unwrap()
                .url,
            "https://registry.example"
        );
        assert!(TrustedRegistryParams::try_parse_from(["remove"]).is_err());
    }
    use crate::util::future::NonDetachingJoinHandle;

    fn public() -> Public {
        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let mut public = Public::init(&account, false, None, None).unwrap();
        public.server_info.registries = Registries::new();
        mutate_public_keys(&mut public, |_| Ok(())).unwrap();
        public
    }

    fn signers<const N: usize>(
        values: [(AnyVerifyingKey, &str); N],
    ) -> BTreeMap<crate::rpc_continuations::Guid, crate::registry::signer::SignerInfo> {
        values
            .into_iter()
            .map(|(key, name)| {
                (
                    crate::rpc_continuations::Guid::new(),
                    crate::registry::signer::SignerInfo {
                        name: name.into(),
                        contact: vec![],
                        keys: BTreeSet::from([key]),
                        gpg_exports: BTreeSet::new(),
                    },
                )
            })
            .collect()
    }

    fn ids<'a>(keys: impl IntoIterator<Item = &'a AnyVerifyingKey>) -> BTreeSet<InternedString> {
        keys.into_iter()
            .map(AnyVerifyingKey::interned_der64)
            .collect()
    }

    fn key(seed: u8) -> AnyVerifyingKey {
        AnyVerifyingKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key())
    }

    #[test]
    fn fresh_public_keys_are_removable_without_observed_listings() {
        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let public = Public::init(&account, false, None, None).unwrap();
        assert!(public.server_info.registry_keys.is_empty());
        assert!(
            public
                .server_info
                .trusted_keys
                .values()
                .all(|key| key.removable)
        );
    }

    #[test]
    fn import_grants_requested_trust_to_new_and_concurrently_imported_keys() {
        let source = include_str!("../sign/fixtures/ed25519-signers.asc");
        for context in [TrustContext::Marketplace, TrustContext::Os] {
            for other_permission in [false, true] {
                let mut public = public();
                let imported = parse_public_keys(&[source.into()]).unwrap();
                let existing = imported.keys.first().unwrap().clone();
                let id = existing.interned_der64();
                mutate_public_keys(&mut public, |keys| {
                    import_individual_keys(keys, "Original".into(), vec![existing].into(), None)
                })
                .unwrap();
                let entry = public.server_info.trusted_keys.get_mut(&id).unwrap();
                match context {
                    TrustContext::Marketplace => {
                        entry.marketplace = false;
                        entry.os = other_permission;
                    }
                    TrustContext::Os => {
                        entry.marketplace = other_permission;
                        entry.os = false;
                    }
                }
                entry.origin = "https://original.example".into();
                entry.origin_registry = Some(RegistryIdentity {
                    name: Some("Original registry".into()),
                    icon: None,
                });
                let before = entry.clone();
                let exports = imported
                    .gpg_exports
                    .iter()
                    .filter(|export| export.identities.contains_key(&id))
                    .cloned()
                    .collect();
                let res = mutate_public_keys(&mut public, |keys| {
                    import_individual_keys(keys, "Replacement".into(), imported, Some(context))
                })
                .unwrap();
                assert_eq!(res.existing, BTreeSet::from([id.clone()]));
                assert_eq!(res.added.len(), 1);
                let (marketplace, os) = match context {
                    TrustContext::Marketplace => (true, other_permission),
                    TrustContext::Os => (other_permission, true),
                };
                assert_eq!(
                    public.server_info.trusted_keys[&id],
                    TrustedKey {
                        marketplace,
                        os,
                        gpg_exports: exports,
                        ..before
                    }
                );
                for added in res.added {
                    let entry = &public.server_info.trusted_keys[&added];
                    assert!(entry.marketplace);
                    assert_eq!(entry.os, matches!(context, TrustContext::Os));
                    assert!(entry.removable);
                    assert_eq!(entry.name, "Replacement");
                    assert_eq!(entry.origin, "manual");
                }
            }
        }
    }

    #[test]
    fn batch_removal_rolls_back_on_protected_missing_or_last_os_keys() {
        for refusal in ["protected", "missing", "last-os"] {
            let mut public = public();
            let mut requested: BTreeSet<_> =
                public.server_info.trusted_keys.keys().cloned().collect();
            if refusal == "protected" {
                let protected = public.server_info.trusted_keys[requested.last().unwrap()]
                    .public_key
                    .clone();
                public.server_info.registries =
                    Registries::from([("https://registry.example".into(), None)]);
                public.server_info.registry_keys.insert(
                    "https://registry.example".into(),
                    BTreeSet::from([protected]),
                );
                mutate_public_keys(&mut public, |_| Ok(())).unwrap();
            } else if refusal == "missing" {
                requested.insert("zz-missing".into());
            }
            let before = to_value(&public).unwrap();
            assert!(mutate_public_keys(&mut public, |keys| remove_keys(keys, requested)).is_err());
            assert_eq!(to_value(&public).unwrap(), before, "{refusal}");
        }
    }

    #[test]
    fn batch_removal_preserves_unrequested_keys_and_accepts_empty_requests() {
        let mut public = public();
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(
                keys,
                "Publisher".into(),
                vec![key(91), key(92)].into(),
                None,
            )
        })
        .unwrap();
        let requested: BTreeSet<_> = public
            .server_info
            .trusted_keys
            .iter()
            .filter(|(_, key)| !key.os)
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(requested.len(), 2);
        let mut expected = public.server_info.trusted_keys.clone();
        for id in &requested {
            expected.remove(id);
        }
        mutate_public_keys(&mut public, |keys| remove_keys(keys, requested)).unwrap();
        assert_eq!(public.server_info.trusted_keys, expected);
        mutate_public_keys(&mut public, |keys| remove_keys(keys, BTreeSet::new())).unwrap();
        assert_eq!(public.server_info.trusted_keys, expected);
    }

    #[test]
    fn alpha_seeds_are_reimported_only_when_a_connected_registry_offers_them() {
        let url = registry_url("https://alpha-registry.start9.com").unwrap();
        let alpha = crate::version::retired_alpha_keys();
        let info = RegistryInfo {
            trust_signers: alpha
                .iter()
                .map(|key| {
                    (
                        crate::rpc_continuations::Guid::new(),
                        crate::registry::signer::SignerInfo {
                            name: "Start9 Alpha".into(),
                            contact: vec![],
                            keys: BTreeSet::from([key.clone()]),
                            gpg_exports: BTreeSet::new(),
                        },
                    )
                })
                .collect(),
            ..Default::default()
        };
        for connected in [false, true] {
            let mut public = public();
            public
                .server_info
                .registry_keys
                .insert("https://alpha-registry.start9.com".into(), alpha.clone());
            if connected {
                public
                    .server_info
                    .registries
                    .insert("https://alpha-registry.start9.com".into(), None);
            }
            let changes = refresh_public(&mut public, &url, &info).unwrap();
            assert_eq!(
                changes.added,
                if connected {
                    ids(&alpha)
                } else {
                    BTreeSet::new()
                }
            );
            for key in &alpha {
                let entry = public.server_info.trusted_keys.get(&key.interned_der64());
                assert_eq!(entry.is_some(), connected);
                if let Some(entry) = entry {
                    assert!(!entry.marketplace && !entry.os);
                }
            }
        }
    }

    #[tokio::test]
    async fn fetched_registry_info_protects_keys_on_connection_and_refresh() {
        let signing_key = key(91);
        let id = signing_key.interned_der64();
        let info = RegistryInfo {
            trust_signers: signers([(signing_key.clone(), "Publisher")]),
            ..Default::default()
        };
        let temp = tempfile::tempdir().unwrap();
        let ctx =
            crate::registry::context::test_context(temp.path().to_owned(), signing_key.clone())
                .await;
        ctx.db
            .mutate(|db| {
                let signers: BTreeSet<_> = info.trust_signers.keys().cloned().collect();
                db.as_admins_mut().ser(&signers)?;
                db.as_index_mut().as_trust_signers_mut().ser(&signers)?;
                db.as_index_mut().as_signers_mut().ser(&info.trust_signers)
            })
            .await
            .result
            .unwrap();
        let router = axum::Router::new().nest("/catalog", crate::registry::registry_router(ctx));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = registry_url(&format!(
            "http://localhost:{}/catalog/",
            listener.local_addr().unwrap().port()
        ))
        .unwrap();
        let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        })
        .into();
        let (client, _proxy) = crate::registry::context::test_utils::socks_client(&url).await;
        let fetched = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            fetch_info(
                &client,
                &url,
                crate::sign::Signer::Key(crate::sign::AnySigningKey::Ed25519(
                    ed25519_dalek::SigningKey::from_bytes(&[71; 32]),
                )),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(fetched.trust_signers, info.trust_signers);

        let mut connected = public();
        assert_eq!(
            connect_public(&mut connected, &url, None, &fetched).unwrap(),
            BTreeSet::from([id.clone()])
        );
        assert!(!connected.server_info.trusted_keys[&id].removable);
        assert!(mutate_public_keys(&mut connected, |keys| remove_key(keys, &id)).is_err());

        let mut existing = public();
        mutate_public_keys(&mut existing, |keys| {
            import_individual_keys(
                keys,
                "Manual".into(),
                vec![signing_key.clone()].into(),
                None,
            )
        })
        .unwrap();
        existing
            .server_info
            .registries
            .insert(url.as_str().trim_end_matches('/').into(), None);
        assert!(existing.server_info.trusted_keys[&id].removable);
        assert!(
            refresh_public(&mut existing, &url, &fetched)
                .unwrap()
                .added
                .is_empty()
        );
        assert_eq!(
            offered_keys(&existing, url.as_str()),
            BTreeSet::from([signing_key])
        );
        assert!(!existing.server_info.trusted_keys[&id].removable);
        let before = to_value(&existing).unwrap();
        assert!(mutate_public_keys(&mut existing, |keys| remove_key(keys, &id)).is_err());
        assert!(
            mutate_public_keys(&mut existing, |keys| {
                remove_keys(keys, BTreeSet::from([id.clone()]))
            })
            .is_err()
        );
        assert_eq!(to_value(&existing).unwrap(), before);

        refresh_public(&mut existing, &url, &RegistryInfo::default()).unwrap();
        mutate_public_keys(&mut existing, |keys| remove_key(keys, &id)).unwrap();
    }

    #[test]
    fn connection_trusts_new_keys_for_packages_only() {
        let mut public = public();
        let url = registry_url("https://registry.example/base/").unwrap();
        let (first, second) = (key(40), key(41));
        let info = RegistryInfo {
            trust_signers: signers([(first.clone(), "First"), (second.clone(), "Second")]),
            ..Default::default()
        };
        let before = public.server_info.trusted_keys.clone();
        let added = connect_public(&mut public, &url, Some("Registry".into()), &info).unwrap();
        assert_eq!(
            added,
            BTreeSet::from([first.interned_der64(), second.interned_der64()])
        );
        assert_eq!(
            public.server_info.registries["https://registry.example/base"],
            Some("Registry".into())
        );
        for key in [&first, &second] {
            let entry = public
                .server_info
                .trusted_keys
                .remove(&key.interned_der64())
                .unwrap();
            assert_eq!(entry.origin, "https://registry.example/base");
            assert!(entry.marketplace && !entry.os && !entry.removable);
        }
        assert_eq!(public.server_info.trusted_keys, before);
    }

    #[test]
    fn registry_provenance_retains_the_first_title_and_logo_after_disconnect() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let signing_key = key(90);
        let id = signing_key.interned_der64();
        let mut info = RegistryInfo {
            name: Some("Original registry".into()),
            icon: Some("data:image/png;base64,aWNvbg==".parse().unwrap()),
            trust_signers: signers([(signing_key.clone(), "Publisher")]),
            ..Default::default()
        };
        connect_public(&mut public, &url, None, &info).unwrap();
        let provenance = public.server_info.trusted_keys[&id]
            .origin_registry
            .clone()
            .unwrap();
        assert_eq!(provenance.name.as_deref(), Some("Original registry"));
        assert_eq!(provenance.icon, info.icon);
        info.name = Some("Renamed registry".into());
        info.icon = None;
        assert!(
            connect_public(&mut public, &url, None, &info)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            remove_public(&mut public, &url).unwrap(),
            BTreeSet::from([id.clone()])
        );
        let entry = &public.server_info.trusted_keys[&id];
        assert_eq!(entry.origin_registry.as_ref(), Some(&provenance));
        assert!(entry.removable && entry.marketplace && !entry.os);
    }

    #[test]
    fn connection_keeps_the_trust_label_and_origin_of_existing_keys() {
        let mut public = public();
        let id = public
            .server_info
            .trusted_keys
            .keys()
            .next()
            .unwrap()
            .clone();
        let entry = public.server_info.trusted_keys.get_mut(&id).unwrap();
        entry.marketplace = false;
        entry.os = true;
        let key = entry.public_key.clone();
        let expected = TrustedKey {
            removable: false,
            ..entry.clone()
        };
        let url = registry_url("https://registry.example").unwrap();
        let info = RegistryInfo {
            trust_signers: signers([(id.parse().unwrap(), "Other name")]),
            ..Default::default()
        };
        assert!(
            connect_public(&mut public, &url, None, &info)
                .unwrap()
                .is_empty()
        );
        assert_eq!(public.server_info.trusted_keys[&id], expected);
        assert_eq!(
            public.server_info.registry_keys["https://registry.example"],
            BTreeSet::from([key])
        );
    }

    #[test]
    fn existing_registries_without_proposals_connect_without_implicit_trust() {
        let mut public = public();
        let saved = public.server_info.trusted_keys.clone();
        let info: RegistryInfo = serde_json::from_value(serde_json::json!({ "name": null, "icon": null, "description": null, "categories": {} })).unwrap();
        assert!(info.trust_signers.is_empty());
        assert!(
            connect_public(
                &mut public,
                &registry_url("https://legacy.example/").unwrap(),
                None,
                &info,
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(public.server_info.trusted_keys, saved);
        assert_eq!(
            public.server_info.registries["https://legacy.example"],
            None
        );
    }

    #[test]
    fn connected_registry_membership_protects_keys_until_the_last_listing_is_removed() {
        let mut public = public();
        let key = key(43);
        let id = key.interned_der64();
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(keys, "Manual".into(), vec![key.clone()].into(), None)
        })
        .unwrap();
        public
            .server_info
            .trusted_keys
            .get_mut(&id)
            .unwrap()
            .marketplace = false;
        let info = RegistryInfo {
            trust_signers: signers([(id.parse().unwrap(), "Listed")]),
            ..Default::default()
        };
        let first = registry_url("https://first.example").unwrap();
        let second = registry_url("https://second.example").unwrap();
        for url in [&first, &second] {
            assert!(
                connect_public(&mut public, url, None, &info)
                    .unwrap()
                    .is_empty()
            );
            assert!(!public.server_info.trusted_keys[&id].removable);
            assert!(!public.server_info.trusted_keys[&id].marketplace);
            assert_eq!(public.server_info.trusted_keys[&id].origin, "manual");
            assert!(mutate_public_keys(&mut public, |keys| remove_key(keys, &id)).is_err());
        }
        assert!(remove_public(&mut public, &first).unwrap().is_empty());
        assert!(!public.server_info.trusted_keys[&id].removable);
        assert!(remove_public(&mut public, &second).unwrap().is_empty());
        assert!(public.server_info.trusted_keys[&id].removable);
        assert!(!public.server_info.trusted_keys[&id].marketplace);
        mutate_public_keys(&mut public, |keys| remove_key(keys, &id)).unwrap();
        assert!(!public.server_info.trusted_keys.contains_key(&id));
    }

    #[test]
    fn refresh_and_removal_collapse_registry_aliases_and_use_the_fetched_name() {
        let mut public = public();
        let key = key(44);
        let id = key.interned_der64();
        public.server_info.registries = Registries::from([
            ("https://REGISTRY.example/".into(), Some("Old".into())),
            ("https://unrelated.example/".into(), None),
        ]);
        public.server_info.registry_keys.insert(
            "https://registry.example/".into(),
            BTreeSet::from([key.clone()]),
        );
        let url = registry_url("https://registry.example/").unwrap();
        let info = RegistryInfo {
            name: Some("Fetched name".into()),
            trust_signers: signers([(id.parse().unwrap(), "Key")]),
            ..Default::default()
        };
        connect_public(&mut public, &url, None, &info).unwrap();
        assert_eq!(public.server_info.registries.len(), 2);
        assert_eq!(
            public.server_info.registries["https://registry.example"],
            Some("Fetched name".into())
        );
        assert_eq!(public.server_info.registry_keys.len(), 1);
        let saved = public.server_info.trusted_keys[&id].clone();
        assert!(!saved.removable);
        assert_eq!(
            remove_public(
                &mut public,
                &registry_url("https://REGISTRY.example/").unwrap(),
            )
            .unwrap(),
            BTreeSet::from([id.clone()])
        );
        assert_eq!(public.server_info.registries.len(), 1);
        assert!(public.server_info.registry_keys.is_empty());
        assert_eq!(
            public.server_info.trusted_keys[&id],
            TrustedKey {
                removable: true,
                ..saved
            }
        );
    }

    fn gpg_info() -> RegistryInfo {
        let imported =
            parse_public_keys(&[include_str!("../sign/fixtures/ed25519-signers.asc").into()])
                .unwrap();
        RegistryInfo {
            name: Some("Original registry".into()),
            trust_signers: BTreeMap::from([(
                crate::rpc_continuations::Guid::new(),
                crate::registry::signer::SignerInfo {
                    name: "Publisher".into(),
                    contact: vec![],
                    keys: imported.keys,
                    gpg_exports: imported.gpg_exports,
                },
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn connection_preserves_full_certificates_and_labels_keys_by_identity() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let info = gpg_info();
        let signer = info.trust_signers.values().next().unwrap();
        connect_public(&mut public, &url, None, &info).unwrap();
        for key in &signer.keys {
            let id = key.interned_der64();
            let entry = &public.server_info.trusted_keys[&id];
            assert_eq!(entry.gpg_exports, signer.gpg_exports);
            assert!(entry.marketplace && !entry.os && !entry.removable);
            assert_eq!(entry.origin, "https://registry.example");
            let user_id = signer
                .gpg_exports
                .iter()
                .find_map(|export| export.identities.get(&id))
                .and_then(|ids| ids.first())
                .unwrap();
            assert!(
                user_id.starts_with(&entry.name),
                "{user_id} / {}",
                entry.name
            );
        }
    }

    #[test]
    fn registry_contact_details_accumulate_across_registries() {
        let mut public = public();
        let signing_key = key(91);
        let id = signing_key.interned_der64();
        for (url, email) in [
            ("https://first.example", "first@example.com"),
            ("https://second.example", "second@example.com"),
        ] {
            let mut info = RegistryInfo {
                trust_signers: signers([(signing_key.clone(), "Publisher")]),
                ..Default::default()
            };
            info.trust_signers.values_mut().next().unwrap().contact =
                vec![crate::registry::signer::ContactInfo::Email(email.into())];
            connect_public(&mut public, &registry_url(url).unwrap(), None, &info).unwrap();
        }
        let entry = &public.server_info.trusted_keys[&id];
        assert_eq!(entry.name, "Publisher");
        assert_eq!(
            entry.contact,
            BTreeSet::from([
                crate::registry::signer::ContactInfo::Email("first@example.com".into()),
                crate::registry::signer::ContactInfo::Email("second@example.com".into()),
            ])
        );
        assert_eq!(entry.origin, "https://first.example");
    }

    #[test]
    fn refresh_imports_disabled_keys_and_preserves_existing_global_permissions_and_provenance() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        connect_public(&mut public, &url, None, &RegistryInfo::default()).unwrap();
        let info = gpg_info();
        let signer = info.trust_signers.values().next().unwrap();
        let manual = signer.keys.first().unwrap();
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(
                keys,
                "Manual name".into(),
                vec![manual.clone()].into(),
                None,
            )
        })
        .unwrap();
        let entry = public
            .server_info
            .trusted_keys
            .get_mut(&manual.interned_der64())
            .unwrap();
        entry.marketplace = false;
        entry.os = true;
        let saved = public.server_info.trusted_keys[&manual.interned_der64()].clone();
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap().added,
            ids(signer.keys.iter().filter(|key| *key != manual))
        );
        for key in &signer.keys {
            let entry = &public.server_info.trusted_keys[&key.interned_der64()];
            assert_eq!(entry.gpg_exports, signer.gpg_exports);
            assert!(!entry.marketplace && !entry.removable);
            if key == manual {
                assert_eq!(entry.name, saved.name);
                assert_eq!(entry.origin, saved.origin);
                assert_eq!(entry.origin_registry, saved.origin_registry);
                assert_eq!(entry.os, saved.os);
            } else {
                assert!(!entry.os);
                assert_eq!(entry.origin_registry.as_ref().unwrap().name, info.name);
            }
        }
        let mut reloaded: Public = from_value(to_value(&public).unwrap()).unwrap();
        assert_eq!(
            refresh_public(&mut reloaded, &url, &info).unwrap(),
            KeyChanges::default()
        );
        assert_eq!(to_value(&reloaded).unwrap(), to_value(&public).unwrap());
    }

    #[test]
    fn discovery_snapshot_tracks_rotation_and_relisting_without_resetting_trust_or_origin() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        connect_public(&mut public, &url, None, &RegistryInfo::default()).unwrap();
        let mut info = gpg_info();
        let listed = info.trust_signers.values().next().unwrap().keys.clone();
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap(),
            KeyChanges {
                added: ids(&listed),
                dropped: BTreeSet::new(),
            }
        );
        let id = listed.first().unwrap().interned_der64();
        let entry = public.server_info.trusted_keys.get_mut(&id).unwrap();
        entry.marketplace = true;
        entry.os = true;
        let saved = entry.clone();
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap(),
            KeyChanges::default()
        );
        assert_eq!(
            refresh_public(&mut public, &url, &RegistryInfo::default()).unwrap(),
            KeyChanges {
                added: BTreeSet::new(),
                dropped: ids(&listed),
            }
        );
        assert_eq!(
            public.server_info.trusted_keys[&id],
            TrustedKey {
                removable: true,
                ..saved.clone()
            }
        );
        info.name = Some("Untrusted replacement name".into());
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap(),
            KeyChanges::default()
        );
        assert_eq!(public.server_info.trusted_keys[&id], saved);
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap(),
            KeyChanges::default()
        );
    }

    #[test]
    fn registry_membership_uses_saved_urls_not_stale_key_snapshots() {
        let mut public = public();
        let url = registry_url("https://registry.example/base/").unwrap();
        public.server_info.registry_keys.insert(
            "https://registry.example/base".into(),
            BTreeSet::from([key(88)]),
        );
        assert!(!is_connected(&public.server_info.registries, url.as_str()));
        let saved = to_value(&public).unwrap();
        assert_eq!(
            refresh_public(&mut public, &url, &gpg_info()).unwrap(),
            KeyChanges::default()
        );
        assert_eq!(to_value(&public).unwrap(), saved);
        let registries = Registries::from([(
            "https://REGISTRY.example/base/".into(),
            Some("Saved".into()),
        )]);
        assert!(is_connected(&registries, url.as_str()));
        assert!(!is_connected(&registries, "https://registry.example/other"));
        assert!(!is_connected(&registries, "https://unknown.example/base"));
    }

    #[test]
    fn in_flight_refresh_does_not_restore_a_removed_registry() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let fetched_info = gpg_info();
        connect_public(&mut public, &url, None, &RegistryInfo::default()).unwrap();
        remove_public(&mut public, &url).unwrap();
        let saved = to_value(&public).unwrap();
        assert_eq!(
            refresh_public(&mut public, &url, &fetched_info).unwrap(),
            KeyChanges::default()
        );
        assert_eq!(to_value(&public).unwrap(), saved);
        assert!(public.server_info.registry_keys.is_empty());
    }

    #[test]
    fn rotation_keeps_shared_keys_protected() {
        let mut public = public();
        let first = registry_url("https://first.example").unwrap();
        let second = registry_url("https://second.example").unwrap();
        let info = RegistryInfo {
            trust_signers: signers([(key(87), "Publisher")]),
            ..Default::default()
        };
        let id = key(87).interned_der64();
        for url in [&first, &second] {
            connect_public(&mut public, url, None, &info).unwrap();
        }
        refresh_public(&mut public, &first, &RegistryInfo::default()).unwrap();
        assert!(!public.server_info.trusted_keys[&id].removable);
        refresh_public(&mut public, &second, &RegistryInfo::default()).unwrap();
        assert!(public.server_info.trusted_keys[&id].removable);
        assert_eq!(
            public.server_info.trusted_keys[&id].origin,
            "https://first.example"
        );
    }

    #[test]
    fn os_registry_selection_trusts_its_keys_for_os_updates() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let (listed, unlisted) = (key(60), key(61));
        assert!(set_os_registry_public(&mut public, &url).is_err());
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(keys, "Manual".into(), vec![unlisted.clone()].into(), None)
        })
        .unwrap();
        let info = RegistryInfo {
            trust_signers: signers([(listed.clone(), "Listed")]),
            ..Default::default()
        };
        connect_public(&mut public, &url, None, &info).unwrap();
        let os_before = enabled_keys(&public.server_info.trusted_keys, TrustContext::Os);
        assert_eq!(
            set_os_registry_public(&mut public, &url).unwrap(),
            BTreeSet::from([listed.interned_der64()])
        );
        assert_eq!(public.server_info.os_registry, url);
        let keys = &public.server_info.trusted_keys;
        assert!(keys[&listed.interned_der64()].os);
        assert!(!keys[&unlisted.interned_der64()].os);
        assert_eq!(
            enabled_keys(keys, TrustContext::Os).len(),
            os_before.len() + 1
        );
        assert!(
            set_os_registry_public(&mut public, &url)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_os_registry_cannot_be_removed_and_removal_reports_only_unoffered_registry_keys() {
        let mut public = public();
        let first = registry_url("https://first.example").unwrap();
        let second = registry_url("https://second.example").unwrap();
        let (shared, own, manual) = (key(70), key(71), key(72));
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(keys, "Manual".into(), vec![manual.clone()].into(), None)
        })
        .unwrap();
        connect_public(
            &mut public,
            &first,
            None,
            &RegistryInfo {
                trust_signers: signers([
                    (shared.clone(), "Shared"),
                    (own.clone(), "Own"),
                    (manual.clone(), "Manual"),
                ]),
                ..Default::default()
            },
        )
        .unwrap();
        connect_public(
            &mut public,
            &second,
            None,
            &RegistryInfo {
                trust_signers: signers([(shared.clone(), "Shared")]),
                ..Default::default()
            },
        )
        .unwrap();
        set_os_registry_public(&mut public, &first).unwrap();
        let before = to_value(&public).unwrap();
        assert!(remove_public(&mut public, &first).is_err());
        assert_eq!(to_value(&public).unwrap(), before);
        set_os_registry_public(&mut public, &second).unwrap();
        assert_eq!(
            remove_public(&mut public, &first).unwrap(),
            BTreeSet::from([own.interned_der64()])
        );
    }

    #[test]
    fn a_new_server_takes_os_updates_from_a_connected_registry() {
        assert!(is_connected(
            &initial_registries(),
            initial_os_registry().as_str()
        ));
    }

    #[test]
    fn builtin_registries_can_be_removed_unless_they_provide_os_updates() {
        let mut public = public();
        ensure_builtin_registries(&mut public.server_info.registries);
        public.server_info.os_registry = "https://registry.start9.com/".parse().unwrap();
        let start9 = registry_url("https://registry.start9.com").unwrap();
        assert!(remove_public(&mut public, &start9).is_err());
        remove_public(
            &mut public,
            &registry_url("https://community-registry.start9.com/").unwrap(),
        )
        .unwrap();
        assert!(!is_connected(
            &public.server_info.registries,
            "https://community-registry.start9.com"
        ));
        public.server_info.os_registry = "https://registry.example/".parse().unwrap();
        remove_public(&mut public, &start9).unwrap();
        assert!(!is_connected(
            &public.server_info.registries,
            "https://registry.start9.com"
        ));
    }

    #[test]
    fn the_last_os_update_key_keeps_its_trust() {
        let mut public = public();
        let ids = public
            .server_info
            .trusted_keys
            .iter()
            .filter(|(_, key)| key.os)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let (last, rest) = ids.split_last().unwrap();
        let distrust = |public: &mut Public, id: &InternedString| {
            let key = public.server_info.trusted_keys[id].clone();
            mutate_public_keys(public, |keys| {
                update_key(
                    keys,
                    UpdateTrustedKeyParams {
                        id: id.clone(),
                        name: key.name,
                        marketplace: key.marketplace,
                        os: false,
                    },
                )
            })
        };
        for id in rest {
            distrust(&mut public, id).unwrap();
        }
        let before = to_value(&public).unwrap();
        assert!(distrust(&mut public, last).is_err());
        assert!(mutate_public_keys(&mut public, |keys| remove_key(keys, last)).is_err());
        assert_eq!(to_value(&public).unwrap(), before);
    }

    #[test]
    fn legacy_registry_discovery_canonicalizes_snapshot_aliases_and_is_atomic_on_invalid_exports() {
        let mut public = public();
        public.server_info.registries =
            Registries::from([("https://REGISTRY.example/".into(), Some("My name".into()))]);
        let url = registry_url("https://registry.example").unwrap();
        let mut info = gpg_info();
        let listed = info.trust_signers.values().next().unwrap().keys.clone();
        let saved = to_value(&public).unwrap();
        let signer = info.trust_signers.values_mut().next().unwrap();
        let mut export = signer.gpg_exports.pop_first().unwrap();
        let armor = export.armored.clone();
        export.armored = "invalid".into();
        signer.gpg_exports.insert(export.clone());
        assert!(refresh_public(&mut public, &url, &info).is_err());
        assert_eq!(to_value(&public).unwrap(), saved);
        let signer = info.trust_signers.values_mut().next().unwrap();
        signer.gpg_exports.clear();
        export.armored = armor;
        signer.gpg_exports.insert(export);
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap().added,
            ids(&listed)
        );
        assert_eq!(
            public.server_info.registries["https://REGISTRY.example/"],
            Some("My name".into())
        );
        assert_eq!(public.server_info.registry_keys.len(), 1);
        assert!(
            public
                .server_info
                .registry_keys
                .contains_key("https://registry.example")
        );
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap(),
            KeyChanges::default()
        );
    }

    #[test]
    fn registry_urls_preserve_base_paths_and_reject_non_registry_components() {
        for value in [
            "file:///tmp/registry",
            "ftp://example.com",
            "https://user@example.com",
            "https://example.com?token=x",
            "https://example.com/#fragment",
        ] {
            assert!(registry_url(value).is_err(), "{value}");
        }
        assert_eq!(
            registry_rpc_url(&registry_url("https://example.com/base/").unwrap())
                .unwrap()
                .as_str(),
            "https://example.com/base/rpc/v0"
        );
        assert_eq!(
            registry_rpc_url(&registry_url("https://example.com/").unwrap())
                .unwrap()
                .as_str(),
            "https://example.com/rpc/v0"
        );
    }
}
