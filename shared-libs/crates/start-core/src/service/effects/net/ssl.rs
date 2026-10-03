use std::collections::BTreeSet;
use std::net::IpAddr;

use imbl_value::InternedString;
use ipnet::IpNet;
use itertools::Itertools;
use openssl::pkey::{PKey, Private};

use crate::HOST_IP;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::util::serde::Pem;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Algorithm {
    Ecdsa,
    Ed25519,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetSslCertificateParams {
    #[ts(type = "string[]")]
    hostnames: BTreeSet<InternedString>,

    algorithm: Option<Algorithm>, //"ecdsa" | "ed25519"

    callback: Option<CallbackId>,
}
pub async fn get_ssl_certificate(
    ctx: EffectContext,
    GetSslCertificateParams {
        hostnames,
        algorithm,
        callback,
    }: GetSslCertificateParams,
) -> Result<Vec<String>, Error> {
    let context = ctx.deref()?;
    let algorithm = algorithm.unwrap_or(Algorithm::Ecdsa);

    let cert = context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let errfn = |h: &str| Error::new(eyre!("unknown hostname: {h}"), ErrorKind::NotFound);
            let entries = db.as_public().as_package_data().as_entries()?;
            let packages = entries.iter().map(|(k, _)| k).collect::<BTreeSet<_>>();
            let allowed_hostnames = entries
                .iter()
                .map(|(_, m)| m.as_hosts().as_entries())
                .flatten_ok()
                .map_ok(|(_, m)| {
                    Ok(m.as_public_domains()
                        .keys()?
                        .into_iter()
                        .chain(m.as_private_domains().keys()?)
                        .chain(
                            m.as_bindings()
                                .de()?
                                .values()
                                .flat_map(|b| b.addresses.available.iter().cloned())
                                .map(|h| h.to_san_hostname()),
                        )
                        .collect::<Vec<InternedString>>())
                })
                .map(|a| a.and_then(|a| a))
                .flatten_ok()
                .try_collect::<_, BTreeSet<_>, _>()?;
            for hostname in &hostnames {
                if let Some(internal) = hostname
                    .strip_suffix(".embassy")
                    .or_else(|| hostname.strip_suffix(".startos"))
                {
                    if !packages.contains(internal) {
                        return Err(errfn(&*hostname));
                    }
                } else if let Ok(ip) = hostname.parse::<IpAddr>() {
                    if IpNet::new(HOST_IP.into(), 24)
                        .with_kind(ErrorKind::ParseNetAddress)?
                        .contains(&ip)
                    {
                        Ok(())
                    } else if db
                        .as_public()
                        .as_server_info()
                        .as_network()
                        .as_gateways()
                        .as_entries()?
                        .into_iter()
                        .flat_map(|(_, net)| net.as_ip_info().transpose_ref())
                        .flat_map(|net| net.as_deref().as_subnets().de().log_err())
                        .flatten()
                        .any(|s| s.addr() == ip)
                    {
                        Ok(())
                    } else {
                        Err(errfn(&*hostname))
                    }?;
                } else {
                    if !allowed_hostnames.contains(hostname) {
                        return Err(errfn(&*hostname));
                    }
                }
            }
            let hostname = db.as_public().as_server_info().as_hostname().de()?;
            let branding = crate::net::ssl::CertBranding::start_os(&hostname);
            db.as_private_mut()
                .as_key_store_mut()
                .as_local_certs_mut()
                .cert_for(&hostnames, &branding)
        })
        .await
        .result?;
    let fullchain = match algorithm {
        Algorithm::Ecdsa => cert.fullchain_nistp256(),
        Algorithm::Ed25519 => cert.fullchain_ed25519(),
    };

    let res = fullchain
        .into_iter()
        .map(|c| c.to_pem())
        .map_ok(String::from_utf8)
        .map(|a| Ok::<_, Error>(a??))
        .try_collect()?;

    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_ssl_certificate(
            ctx,
            hostnames,
            cert,
            algorithm,
            CallbackHandler::new(&context, callback),
        );
    }

    Ok(res)
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetSslKeyParams {
    #[ts(type = "string[]")]
    hostnames: BTreeSet<InternedString>,

    algorithm: Option<Algorithm>, //"ecdsa" | "ed25519"
}
pub async fn get_ssl_key(
    context: EffectContext,
    GetSslKeyParams {
        hostnames,
        algorithm,
    }: GetSslKeyParams,
) -> Result<Pem<PKey<Private>>, Error> {
    let context = context.deref()?;
    let package_id = &context.seed.id;
    let algorithm = algorithm.unwrap_or(Algorithm::Ecdsa);
    let container_ip = if let Some(lxc) = context.seed.persistent_container.lxc_container.get() {
        Some(lxc.ip().await?)
    } else {
        None
    };

    let cert = context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let errfn = |h: &str| Error::new(eyre!("unknown hostname: {h}"), ErrorKind::NotFound);
            let mut allowed_hostnames = db
                .as_public()
                .as_package_data()
                .as_idx(package_id)
                .into_iter()
                .map(|m| m.as_hosts().as_entries())
                .flatten_ok()
                .map_ok(|(_, m)| {
                    Ok(m.as_public_domains()
                        .keys()?
                        .into_iter()
                        .chain(m.as_private_domains().keys()?)
                        .chain(
                            m.as_bindings()
                                .de()?
                                .values()
                                .flat_map(|b| b.addresses.available.iter().cloned())
                                .map(|h| h.to_san_hostname()),
                        )
                        .collect::<Vec<InternedString>>())
                })
                .map(|a| a.and_then(|a| a))
                .flatten_ok()
                .try_collect::<_, BTreeSet<_>, _>()?;
            allowed_hostnames.extend(container_ip.as_ref().map(InternedString::from_display));
            for hostname in &hostnames {
                if let Some(internal) = hostname
                    .strip_suffix(".embassy")
                    .or_else(|| hostname.strip_suffix(".startos"))
                {
                    if internal != &**package_id {
                        return Err(errfn(&*hostname));
                    }
                } else {
                    if !allowed_hostnames.contains(hostname) {
                        return Err(errfn(&*hostname));
                    }
                }
            }
            let hostname = db.as_public().as_server_info().as_hostname().de()?;
            let branding = crate::net::ssl::CertBranding::start_os(&hostname);
            db.as_private_mut()
                .as_key_store_mut()
                .as_local_certs_mut()
                .cert_for(&hostnames, &branding)
        })
        .await
        .result?;
    let key = match algorithm {
        Algorithm::Ecdsa => cert.leaf.keys.nistp256,
        Algorithm::Ed25519 => cert.leaf.keys.ed25519,
    };

    Ok(Pem(key))
}
