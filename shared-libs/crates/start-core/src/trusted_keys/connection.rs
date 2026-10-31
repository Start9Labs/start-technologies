use http::HeaderMap;
use serde::{Deserialize, Serialize};
use url::Url;

use super::*;
use crate::db::model::public::Public;
use crate::middleware::auth::signature::call_remote_with_client;
use crate::registry::info::RegistryInfo;
use crate::registry::package::promote::registry_rpc_url;

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPreviewParams {
    pub url: String,
}

rpc_toolkit::reflect_ts!(RegistryPreviewParams);

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRegistryParams {
    pub url: String,
    pub name: Option<String>,
    /// Explicit consent to enable Marketplace trust for these proposed keys.
    pub approved_keys: BTreeSet<AnyVerifyingKey>,
}

rpc_toolkit::reflect_ts!(ConnectRegistryParams);

fn registry_url(value: &str) -> Result<Url, Error> {
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

pub(super) fn same_registry(first: &str, second: &str) -> bool {
    match (registry_url(first), registry_url(second)) {
        (Ok(first), Ok(second)) => first == second,
        _ => false,
    }
}

async fn fetch_info(ctx: &RpcContext, url: &Url) -> Result<RegistryInfo, Error> {
    let value = call_remote_with_client(
        None,
        ctx.client.get(),
        registry_rpc_url(url)?,
        HeaderMap::new(),
        "registry.info",
        imbl_value::json!({}),
    )
    .await?;
    from_value(value)
}

pub async fn preview(
    ctx: RpcContext,
    params: RegistryPreviewParams,
) -> Result<RegistryInfo, Error> {
    fetch_info(&ctx, &registry_url(&params.url)?).await
}

pub async fn connect(ctx: RpcContext, params: ConnectRegistryParams) -> Result<(), Error> {
    let url = registry_url(&params.url)?;
    let info = fetch_info(&ctx, &url).await?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut().mutate(|public| {
                connect_public(public, &url, params.name, &params.approved_keys, &info)
            })
        })
        .await
        .result
}

pub async fn remove(ctx: RpcContext, params: RegistryPreviewParams) -> Result<(), Error> {
    let url = registry_url(&params.url)?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| remove_public(public, &url))
        })
        .await
        .result
}

fn forget_registry(
    ui: &mut Value,
    snapshots: &mut BTreeMap<InternedString, BTreeSet<AnyVerifyingKey>>,
    url: &str,
) -> Result<(), Error> {
    let object = ui.as_object_mut().ok_or_else(invalid_group)?;
    if !object.contains_key("registries") {
        object.insert("registries".into(), imbl_value::json!({}));
    }
    let registries = object
        .get_mut("registries")
        .unwrap()
        .as_object_mut()
        .ok_or_else(invalid_group)?;
    let aliases = registries
        .keys()
        .filter(|saved| same_registry(saved, url))
        .cloned()
        .collect::<Vec<_>>();
    for alias in aliases {
        registries.remove(&alias);
    }
    snapshots.retain(|saved, _| !same_registry(saved, url));
    Ok(())
}

fn remove_public(public: &mut Public, url: &Url) -> Result<(), Error> {
    let mut ui = public.ui.clone();
    let mut snapshots = public.server_info.registry_keys.clone();
    forget_registry(&mut ui, &mut snapshots, url.as_str())?;
    let mut keys = individual_keys(&public.server_info.trusted_keys)?;
    protect_registry_keys(&mut keys, &snapshots, &ui);
    public.ui = ui;
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(())
}

fn connect_public(
    public: &mut Public,
    url: &Url,
    name: Option<String>,
    approved: &BTreeSet<AnyVerifyingKey>,
    info: &RegistryInfo,
) -> Result<(), Error> {
    for key in approved {
        if !info.trust_keys.contains_key(key) {
            return Err(Error::new(
                eyre!("{}", t!("trusted-keys.proposal-changed")),
                ErrorKind::InvalidRequest,
            ));
        }
    }
    let origin = url.as_str().trim_end_matches('/');
    let mut keys = individual_keys(&public.server_info.trusted_keys)?;
    for key in approved {
        let id = key.interned_der64();
        let label = info.trust_keys[key].trim();
        let existing = keys.contains_key(&id);
        import_individual_keys(
            &mut keys,
            if label.is_empty() {
                id.to_string()
            } else {
                label.to_owned()
            },
            vec![key.clone()].into(),
            TrustContext::Marketplace,
            origin,
        )?;
        let key = keys.get_mut(&id).unwrap();
        key.marketplace = true;
        if !existing {
            key.origin_registry = Some(RegistryIdentity {
                name: info.name.clone(),
                icon: info.icon.clone(),
            });
        }
    }
    let proposed = info.trust_keys.keys().cloned().collect();
    let mut ui = public.ui.clone();
    let mut snapshots = public.server_info.registry_keys.clone();
    forget_registry(&mut ui, &mut snapshots, origin)?;
    ui.as_object_mut()
        .unwrap()
        .get_mut("registries")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(
            origin.into(),
            to_value(&name.or_else(|| info.name.clone()))?,
        );
    snapshots.insert(origin.into(), proposed);
    protect_registry_keys(&mut keys, &snapshots, &ui);
    public.ui = ui;
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public() -> Public {
        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let mut public = Public::init(&account, false, None, None).unwrap();
        public.ui = imbl_value::json!({ "registries": {} });
        public
    }

    fn key(seed: u8) -> AnyVerifyingKey {
        AnyVerifyingKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key())
    }

    #[test]
    fn registry_approval_accepts_exact_subset_without_os_escalation() {
        let mut public = public();
        let url = registry_url("https://registry.example/base/").unwrap();
        let (approved, unapproved) = (key(40), key(41));
        let info = RegistryInfo {
            trust_keys: BTreeMap::from([
                (approved.clone(), "Approved".into()),
                (unapproved.clone(), "Unapproved".into()),
            ]),
            ..Default::default()
        };
        let before = public.server_info.trusted_keys.clone();
        connect_public(
            &mut public,
            &url,
            Some("Registry".into()),
            &BTreeSet::from([approved.clone()]),
            &info,
        )
        .unwrap();
        assert_eq!(
            public.ui["registries"]["https://registry.example/base"],
            imbl_value::json!("Registry")
        );
        let entry = public
            .server_info
            .trusted_keys
            .remove(&approved.interned_der64())
            .unwrap();
        assert_eq!(entry.origin, "https://registry.example/base");
        assert!(entry.marketplace && !entry.os && !entry.removable);
        assert!(
            !public
                .server_info
                .trusted_keys
                .contains_key(&unapproved.interned_der64())
        );
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
            trust_keys: BTreeMap::from([(signing_key.clone(), "Publisher".into())]),
            ..Default::default()
        };
        let approved = BTreeSet::from([signing_key]);
        connect_public(&mut public, &url, None, &approved, &info).unwrap();
        let provenance = public.server_info.trusted_keys[&id]
            .origin_registry
            .clone()
            .unwrap();
        assert_eq!(provenance.name.as_deref(), Some("Original registry"));
        assert_eq!(provenance.icon, info.icon);
        info.name = Some("Renamed registry".into());
        info.icon = None;
        connect_public(&mut public, &url, None, &approved, &info).unwrap();
        remove_public(&mut public, &url).unwrap();
        let entry = &public.server_info.trusted_keys[&id];
        assert_eq!(entry.origin_registry.as_ref(), Some(&provenance));
        assert!(entry.removable && entry.marketplace && !entry.os);
    }

    #[test]
    fn existing_disabled_keys_require_explicit_approval_and_keep_origin_and_os_selection() {
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
        let key = entry.public_keys.first().unwrap().clone();
        let saved = entry.clone();
        let url = registry_url("https://registry.example").unwrap();
        let info = RegistryInfo {
            trust_keys: BTreeMap::from([(id.parse().unwrap(), "Other name".into())]),
            ..Default::default()
        };
        connect_public(&mut public, &url, None, &BTreeSet::new(), &info).unwrap();
        assert_eq!(public.server_info.trusted_keys[&id], saved);
        connect_public(&mut public, &url, None, &BTreeSet::from([key]), &info).unwrap();
        assert_eq!(
            public.server_info.trusted_keys[&id],
            TrustedKeyring {
                marketplace: true,
                ..saved
            }
        );
    }

    #[test]
    fn stale_approval_or_invalid_ui_leaves_registry_and_trust_unchanged() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let approved = key(42);
        let before = to_value(&public).unwrap();
        assert!(
            connect_public(
                &mut public,
                &url,
                None,
                &BTreeSet::from([approved.clone()]),
                &RegistryInfo::default()
            )
            .is_err()
        );
        assert_eq!(to_value(&public).unwrap(), before);
        public.ui = imbl_value::json!({ "registries": false });
        let before = to_value(&public).unwrap();
        let info = RegistryInfo {
            trust_keys: BTreeMap::from([(approved.clone(), "New key".into())]),
            ..Default::default()
        };
        assert!(
            connect_public(&mut public, &url, None, &BTreeSet::from([approved]), &info).is_err()
        );
        assert_eq!(to_value(&public).unwrap(), before);
    }

    #[test]
    fn existing_registries_without_proposals_connect_without_implicit_trust() {
        let mut public = public();
        let saved = public.server_info.trusted_keys.clone();
        let info: RegistryInfo = serde_json::from_value(serde_json::json!({ "name": null, "icon": null, "description": null, "categories": {} })).unwrap();
        assert!(info.trust_keys.is_empty());
        connect_public(
            &mut public,
            &registry_url("https://legacy.example/").unwrap(),
            None,
            &BTreeSet::new(),
            &info,
        )
        .unwrap();
        assert_eq!(public.server_info.trusted_keys, saved);
        assert_eq!(
            public.ui["registries"]["https://legacy.example"],
            Value::Null
        );
    }

    #[test]
    fn connected_registry_membership_protects_keys_until_the_last_listing_is_removed() {
        let mut public = public();
        let key = key(43);
        let id = key.interned_der64();
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(
                keys,
                "Manual".into(),
                vec![key.clone()].into(),
                TrustContext::Marketplace,
                "manual",
            )
        })
        .unwrap();
        public
            .server_info
            .trusted_keys
            .get_mut(&id)
            .unwrap()
            .marketplace = false;
        let info = RegistryInfo {
            trust_keys: BTreeMap::from([(id.parse().unwrap(), "Listed".into())]),
            ..Default::default()
        };
        let first = registry_url("https://first.example").unwrap();
        let second = registry_url("https://second.example").unwrap();
        for url in [&first, &second] {
            connect_public(&mut public, url, None, &BTreeSet::new(), &info).unwrap();
            assert!(!public.server_info.trusted_keys[&id].removable);
            assert!(!public.server_info.trusted_keys[&id].marketplace);
            assert_eq!(public.server_info.trusted_keys[&id].origin, "manual");
            assert!(mutate_public_keys(&mut public, |keys| remove_group(keys, &id)).is_err());
        }
        remove_public(&mut public, &first).unwrap();
        assert!(!public.server_info.trusted_keys[&id].removable);
        remove_public(&mut public, &second).unwrap();
        assert!(public.server_info.trusted_keys[&id].removable);
        assert!(!public.server_info.trusted_keys[&id].marketplace);
        assert!(
            public
                .server_info
                .trusted_keys
                .values()
                .filter(|entry| entry.origin == "system")
                .all(|entry| !entry.removable)
        );
        mutate_public_keys(&mut public, |keys| remove_group(keys, &id)).unwrap();
        assert!(!public.server_info.trusted_keys.contains_key(&id));
    }

    #[test]
    fn refresh_and_removal_collapse_registry_aliases_and_use_the_fetched_name() {
        let mut public = public();
        let key = key(44);
        let id = key.interned_der64();
        public.ui = imbl_value::json!({ "registries": { "https://REGISTRY.example/": "Old", "https://unrelated.example/": null } });
        public.server_info.registry_keys.insert(
            "https://registry.example/".into(),
            BTreeSet::from([key.clone()]),
        );
        let url = registry_url("https://registry.example/").unwrap();
        let info = RegistryInfo {
            name: Some("Fetched name".into()),
            trust_keys: BTreeMap::from([(id.parse().unwrap(), "Key".into())]),
            ..Default::default()
        };
        connect_public(&mut public, &url, None, &BTreeSet::from([key]), &info).unwrap();
        assert_eq!(public.ui["registries"].as_object().unwrap().len(), 2);
        assert_eq!(
            public.ui["registries"]["https://registry.example"],
            imbl_value::json!("Fetched name")
        );
        assert_eq!(public.server_info.registry_keys.len(), 1);
        let saved = public.server_info.trusted_keys[&id].clone();
        assert!(!saved.removable);
        remove_public(
            &mut public,
            &registry_url("https://REGISTRY.example/").unwrap(),
        )
        .unwrap();
        assert_eq!(public.ui["registries"].as_object().unwrap().len(), 1);
        assert!(public.server_info.registry_keys.is_empty());
        assert_eq!(
            public.server_info.trusted_keys[&id],
            TrustedKeyring {
                removable: true,
                ..saved
            }
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
