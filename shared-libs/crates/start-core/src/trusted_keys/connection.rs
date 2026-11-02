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

pub(super) fn is_connected(ui: &Value, url: &str) -> bool {
    ui.get("registries")
        .and_then(Value::as_object)
        .is_some_and(|registries| registries.keys().any(|saved| same_registry(saved, url)))
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

pub async fn refresh(
    ctx: RpcContext,
    params: RegistryPreviewParams,
) -> Result<BTreeSet<AnyVerifyingKey>, Error> {
    let url = registry_url(&params.url)?;
    let ui = ctx.db.peek().await.as_public().as_ui().de()?;
    if !is_connected(&ui, url.as_str()) {
        return Ok(BTreeSet::new());
    }
    let info = fetch_info(&ctx, &url).await?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| refresh_public(public, &url, &info))
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
    let object = ui.as_object_mut().ok_or_else(invalid_key)?;
    if !object.contains_key("registries") {
        object.insert("registries".into(), imbl_value::json!({}));
    }
    let registries = object
        .get_mut("registries")
        .unwrap()
        .as_object_mut()
        .ok_or_else(invalid_key)?;
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
    let mut keys = public.server_info.trusted_keys.clone();
    protect_registry_keys(&mut keys, &snapshots, &ui);
    public.ui = ui;
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(())
}

fn proposed_keys(info: &RegistryInfo, origin: &str) -> Result<TrustedKeys, Error> {
    let mut proposal = TrustedKeys::new();
    for signer in info.trust_signers.values() {
        let mut imported = split_import(
            signer.name.clone(),
            ImportedPublicKeys {
                keys: signer.keys.clone(),
                gpg_exports: signer.gpg_exports.clone(),
            },
        )?;
        for (id, key) in &mut imported {
            if key.name.trim().is_empty() {
                key.name = id.to_string();
            }
            key.origin = origin.into();
            key.origin_registry = Some(RegistryIdentity {
                name: info.name.clone(),
                icon: info.icon.clone(),
            });
        }
        merge_individual_keys(&mut proposal, imported);
    }
    Ok(proposal)
}

fn connect_public(
    public: &mut Public,
    url: &Url,
    name: Option<String>,
    approved: &BTreeSet<AnyVerifyingKey>,
    info: &RegistryInfo,
) -> Result<(), Error> {
    let origin = url.as_str().trim_end_matches('/');
    let proposal = proposed_keys(info, origin)?;
    for key in approved {
        if !proposal.contains_key(&key.interned_der64()) {
            return Err(Error::new(
                eyre!("{}", t!("trusted-keys.proposal-changed")),
                ErrorKind::InvalidRequest,
            ));
        }
    }
    let proposed = proposal
        .values()
        .map(|key| key.public_key.clone())
        .collect();
    let mut keys = public.server_info.trusted_keys.clone();
    merge_individual_keys(&mut keys, proposal);
    for key in approved {
        keys.get_mut(&key.interned_der64()).unwrap().marketplace = true;
    }
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

fn refresh_public(
    public: &mut Public,
    url: &Url,
    info: &RegistryInfo,
) -> Result<BTreeSet<AnyVerifyingKey>, Error> {
    let origin = url.as_str().trim_end_matches('/');
    if !is_connected(&public.ui, origin) {
        return Ok(BTreeSet::new());
    }
    let proposal = proposed_keys(info, origin)?;
    let proposed: BTreeSet<_> = proposal
        .values()
        .map(|key| key.public_key.clone())
        .collect();
    let mut snapshots = public.server_info.registry_keys.clone();
    let previous: BTreeSet<_> = snapshots
        .iter()
        .filter(|(saved, _)| same_registry(saved, origin))
        .flat_map(|(_, keys)| keys.iter().cloned())
        .collect();
    let newly_listed = proposed.difference(&previous).cloned().collect();
    snapshots.retain(|saved, _| !same_registry(saved, origin));
    snapshots.insert(origin.into(), proposed);
    let mut keys = public.server_info.trusted_keys.clone();
    merge_individual_keys(&mut keys, proposal);
    protect_registry_keys(&mut keys, &snapshots, &public.ui);
    public.server_info.registry_keys = snapshots;
    public.server_info.trusted_keys = keys;
    Ok(newly_listed)
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

    fn key(seed: u8) -> AnyVerifyingKey {
        AnyVerifyingKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key())
    }

    #[test]
    fn registry_approval_accepts_exact_subset_without_os_escalation() {
        let mut public = public();
        let url = registry_url("https://registry.example/base/").unwrap();
        let (approved, unapproved) = (key(40), key(41));
        let info = RegistryInfo {
            trust_signers: signers([
                (approved.clone(), "Approved"),
                (unapproved.clone(), "Unapproved"),
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
        let disabled = public
            .server_info
            .trusted_keys
            .remove(&unapproved.interned_der64())
            .unwrap();
        assert!(!disabled.marketplace && !disabled.os && !disabled.removable);
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
        let key = entry.public_key.clone();
        let saved = entry.clone();
        let url = registry_url("https://registry.example").unwrap();
        let info = RegistryInfo {
            trust_signers: signers([(id.parse().unwrap(), "Other name")]),
            ..Default::default()
        };
        connect_public(&mut public, &url, None, &BTreeSet::new(), &info).unwrap();
        assert_eq!(public.server_info.trusted_keys[&id], saved);
        connect_public(&mut public, &url, None, &BTreeSet::from([key]), &info).unwrap();
        assert_eq!(
            public.server_info.trusted_keys[&id],
            TrustedKey {
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
            trust_signers: signers([(approved.clone(), "New key")]),
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
        assert!(info.trust_signers.is_empty());
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
            trust_signers: signers([(id.parse().unwrap(), "Listed")]),
            ..Default::default()
        };
        let first = registry_url("https://first.example").unwrap();
        let second = registry_url("https://second.example").unwrap();
        for url in [&first, &second] {
            connect_public(&mut public, url, None, &BTreeSet::new(), &info).unwrap();
            assert!(!public.server_info.trusted_keys[&id].removable);
            assert!(!public.server_info.trusted_keys[&id].marketplace);
            assert_eq!(public.server_info.trusted_keys[&id].origin, "manual");
            assert!(mutate_public_keys(&mut public, |keys| remove_key(keys, &id)).is_err());
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
        mutate_public_keys(&mut public, |keys| remove_key(keys, &id)).unwrap();
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
            trust_signers: signers([(id.parse().unwrap(), "Key")]),
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
    fn connection_preserves_full_certificates_without_granting_os_trust() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let info = gpg_info();
        let signer = info.trust_signers.values().next().unwrap();
        let approved = signer.keys.first().unwrap();
        connect_public(
            &mut public,
            &url,
            None,
            &BTreeSet::from([approved.clone()]),
            &info,
        )
        .unwrap();
        for key in &signer.keys {
            let entry = &public.server_info.trusted_keys[&key.interned_der64()];
            assert_eq!(entry.gpg_exports, signer.gpg_exports);
            assert_eq!(entry.marketplace, key == approved);
            assert!(!entry.os && !entry.removable);
            assert_eq!(entry.origin, "https://registry.example");
        }
    }

    #[test]
    fn refresh_imports_disabled_keys_and_preserves_existing_global_permissions_and_provenance() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        connect_public(
            &mut public,
            &url,
            None,
            &BTreeSet::new(),
            &RegistryInfo::default(),
        )
        .unwrap();
        let info = gpg_info();
        let signer = info.trust_signers.values().next().unwrap();
        let manual = signer.keys.first().unwrap();
        mutate_public_keys(&mut public, |keys| {
            import_individual_keys(
                keys,
                "Manual name".into(),
                vec![manual.clone()].into(),
                TrustContext::Os,
                "manual",
            )
        })
        .unwrap();
        public
            .server_info
            .trusted_keys
            .get_mut(&manual.interned_der64())
            .unwrap()
            .marketplace = false;
        let saved = public.server_info.trusted_keys[&manual.interned_der64()].clone();
        assert_eq!(
            refresh_public(&mut public, &url, &info).unwrap(),
            signer.keys
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
        assert!(
            refresh_public(&mut reloaded, &url, &info)
                .unwrap()
                .is_empty()
        );
        assert_eq!(to_value(&reloaded).unwrap(), to_value(&public).unwrap());
    }

    #[test]
    fn discovery_snapshot_tracks_rotation_and_relisting_without_resetting_trust_or_origin() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        connect_public(
            &mut public,
            &url,
            None,
            &BTreeSet::new(),
            &RegistryInfo::default(),
        )
        .unwrap();
        let mut info = gpg_info();
        let listed = info.trust_signers.values().next().unwrap().keys.clone();
        assert_eq!(refresh_public(&mut public, &url, &info).unwrap(), listed);
        let id = listed.first().unwrap().interned_der64();
        let entry = public.server_info.trusted_keys.get_mut(&id).unwrap();
        entry.marketplace = true;
        entry.os = true;
        let saved = entry.clone();
        assert!(refresh_public(&mut public, &url, &info).unwrap().is_empty());
        assert!(
            refresh_public(&mut public, &url, &RegistryInfo::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            public.server_info.trusted_keys[&id],
            TrustedKey {
                removable: true,
                ..saved.clone()
            }
        );
        info.name = Some("Untrusted replacement name".into());
        assert_eq!(refresh_public(&mut public, &url, &info).unwrap(), listed);
        assert_eq!(public.server_info.trusted_keys[&id], saved);
        assert!(refresh_public(&mut public, &url, &info).unwrap().is_empty());
    }

    #[test]
    fn registry_membership_uses_saved_urls_not_stale_key_snapshots() {
        let mut public = public();
        let url = registry_url("https://registry.example/base/").unwrap();
        public.server_info.registry_keys.insert(
            "https://registry.example/base".into(),
            BTreeSet::from([key(88)]),
        );
        assert!(!is_connected(&public.ui, url.as_str()));
        let saved = to_value(&public).unwrap();
        assert!(
            refresh_public(&mut public, &url, &gpg_info())
                .unwrap()
                .is_empty()
        );
        assert_eq!(to_value(&public).unwrap(), saved);
        public.ui =
            imbl_value::json!({ "registries": { "https://REGISTRY.example/base/": "Saved" } });
        assert!(is_connected(&public.ui, url.as_str()));
        assert!(!is_connected(&public.ui, "https://registry.example/other"));
        assert!(!is_connected(&public.ui, "https://unknown.example/base"));
    }

    #[test]
    fn in_flight_refresh_does_not_restore_a_removed_registry() {
        let mut public = public();
        let url = registry_url("https://registry.example").unwrap();
        let fetched_info = gpg_info();
        connect_public(
            &mut public,
            &url,
            None,
            &BTreeSet::new(),
            &RegistryInfo::default(),
        )
        .unwrap();
        remove_public(&mut public, &url).unwrap();
        let saved = to_value(&public).unwrap();
        assert!(
            refresh_public(&mut public, &url, &fetched_info)
                .unwrap()
                .is_empty()
        );
        assert_eq!(to_value(&public).unwrap(), saved);
        assert!(public.server_info.registry_keys.is_empty());
    }

    #[test]
    fn rotation_keeps_shared_and_system_keys_protected() {
        let mut public = public();
        let first = registry_url("https://first.example").unwrap();
        let second = registry_url("https://second.example").unwrap();
        let info = RegistryInfo {
            trust_signers: signers([(key(87), "Publisher")]),
            ..Default::default()
        };
        let id = key(87).interned_der64();
        for url in [&first, &second] {
            connect_public(&mut public, url, None, &BTreeSet::new(), &info).unwrap();
        }
        refresh_public(&mut public, &first, &RegistryInfo::default()).unwrap();
        assert!(!public.server_info.trusted_keys[&id].removable);
        refresh_public(&mut public, &second, &RegistryInfo::default()).unwrap();
        assert!(public.server_info.trusted_keys[&id].removable);
        assert_eq!(
            public.server_info.trusted_keys[&id].origin,
            "https://first.example"
        );
        assert!(
            public
                .server_info
                .trusted_keys
                .values()
                .filter(|entry| entry.origin == "system")
                .all(|entry| !entry.removable)
        );
    }

    #[test]
    fn legacy_registry_discovery_canonicalizes_snapshot_aliases_and_is_atomic_on_invalid_exports() {
        let mut public = public();
        public.ui = imbl_value::json!({ "registries": { "https://REGISTRY.example/": "My name" } });
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
        assert_eq!(refresh_public(&mut public, &url, &info).unwrap(), listed);
        assert_eq!(
            public.ui["registries"]["https://REGISTRY.example/"],
            imbl_value::json!("My name")
        );
        assert_eq!(public.server_info.registry_keys.len(), 1);
        assert!(
            public
                .server_info
                .registry_keys
                .contains_key("https://registry.example")
        );
        assert!(refresh_public(&mut public, &url, &info).unwrap().is_empty());
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
