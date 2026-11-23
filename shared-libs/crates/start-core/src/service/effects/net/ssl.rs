use std::collections::BTreeSet;
use std::net::IpAddr;

use imbl_value::InternedString;
use ipnet::IpNet;
use itertools::Itertools;
use openssl::pkey::{PKey, Private};

use crate::HOST_IP;
use crate::net::dns::OS_HOSTNAME;
use crate::net::host::Host;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::util::serde::Pem;

#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, visit_rs::VisitVariants, PartialEq, Eq,
)]
#[serde(rename_all = "camelCase")]
pub enum Algorithm {
    Ecdsa,
    Ed25519,
}

rpc_toolkit::reflect_ts!(Algorithm);
rpc_toolkit::ts_export!(Algorithm, namespaces = [""]);

// Services may certify the bridge hostname and retrieve its key.
fn allowed_hostnames<'a>(
    hosts: impl IntoIterator<Item = Result<&'a Model<Host>, Error>>,
) -> Result<BTreeSet<InternedString>, Error> {
    let mut names = BTreeSet::from([InternedString::from_static(OS_HOSTNAME)]);
    for host in hosts {
        let host = host?;
        names.extend(host.as_public_domains().keys()?);
        names.extend(host.as_private_domains().keys()?);
        names.extend(
            host.as_bindings()
                .de()?
                .values()
                .flat_map(|binding| binding.addresses.available.iter())
                .map(|address| address.to_san_hostname()),
        );
    }
    Ok(names)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct GetSslCertificateParams {
    hostnames: BTreeSet<InternedString>,

    algorithm: Option<Algorithm>,

    callback: Option<CallbackId>,
}

rpc_toolkit::reflect_ts!(GetSslCertificateParams);
rpc_toolkit::ts_export!(GetSslCertificateParams, namespaces = [""]);
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
            let allowed_hostnames = allowed_hostnames(
                entries
                    .iter()
                    .map(|(_, m)| m.as_hosts().as_entries())
                    .flatten_ok()
                    .map_ok(|(_, m)| m),
            )?;
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

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct GetSslKeyParams {
    hostnames: BTreeSet<InternedString>,

    algorithm: Option<Algorithm>,
}

rpc_toolkit::reflect_ts!(GetSslKeyParams);
rpc_toolkit::ts_export!(GetSslKeyParams, namespaces = [""]);
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
            let mut allowed_hostnames = allowed_hostnames(
                db.as_public()
                    .as_package_data()
                    .as_idx(package_id)
                    .into_iter()
                    .map(|m| m.as_hosts().as_entries())
                    .flatten_ok()
                    .map_ok(|(_, m)| m),
            )?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_and_key_names_include_the_shared_bridge_hostname() {
        let names = allowed_hostnames(std::iter::empty()).unwrap();
        assert_eq!(
            names,
            BTreeSet::from([InternedString::from_static(OS_HOSTNAME)]),
        );

        let mut host = Host::default();
        host.private_domains
            .insert("provider.internal".into(), BTreeSet::new());
        let host = Model::<Host>::new(&host).unwrap();
        let names = allowed_hostnames([Ok(&host)]).unwrap();
        assert_eq!(
            names,
            BTreeSet::from([
                InternedString::from_static(OS_HOSTNAME),
                InternedString::from("provider.internal"),
            ]),
        );
        assert!(!names.contains("unrelated.internal"));
    }
}
