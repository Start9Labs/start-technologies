use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use clap::Parser;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use patch_db::Dump;
use rpc_toolkit::{
    Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async, from_fn_async_local,
};
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::ClientHello;

use crate::context::CliContext;
use crate::net::ssl::{CertBranding, SANInfo, root_ca_start_time};
use crate::net::tls::{TlsHandler, TlsHandlerAction};
use crate::net::web_server::Accept;
use crate::prelude::*;
use crate::tunnel::auth::SetPasswordParams;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::{PortForward, PortForwards, TunnelDatabase};
use crate::util::serde::{Der64, HandlerExtSerde, Pem, display_serializable};
use crate::util::tui::{choose, parse_as, prompt, prompt_multiline};

pub(crate) mod listener;

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct WebserverInfo {
    pub enabled: bool,
    pub listen: Option<SocketAddr>,
    pub certificate: Option<TunnelCertData>,
}

impl WebserverInfo {
    pub(crate) fn reserves_plain_forward(&self, source: SocketAddrV4, count: u16) -> bool {
        self.enabled
            && self
                .listen
                .and_then(ipv4_listener_addr)
                .is_some_and(|listen| {
                    (listen.ip().is_unspecified() || listen.ip() == source.ip())
                        && source.port() <= listen.port()
                        && listen.port() <= source.port().saturating_add(count.saturating_sub(1))
                })
    }

    pub(crate) fn check_plain_forward(
        &self,
        source: SocketAddrV4,
        count: u16,
    ) -> Result<(), Error> {
        if self.reserves_plain_forward(source, count) {
            return Err(Error::new(
                eyre!(t!("tunnel.web.ui-port-reserved")),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok(())
    }

    fn check_forward_conflicts(&self, forwards: &PortForwards) -> Result<(), Error> {
        for (source, forward) in &forwards.0 {
            let enabled = match forward {
                PortForward::Dnat { enabled, .. } => *enabled,
                PortForward::Sni { fallback, .. } => fallback.as_ref().is_some_and(|f| f.enabled),
            };
            if enabled
                && forward.transport(source.clone()) != crate::net::transport::Transport::udp()
                && self.reserves_plain_forward(source.address, forward.port_span())
            {
                return Err(Error::new(
                    eyre!(t!("tunnel.web.listen-port-forwarded")),
                    ErrorKind::InvalidRequest,
                ));
            }
        }
        Ok(())
    }
}

fn ipv4_listener_addr(listen: SocketAddr) -> Option<SocketAddrV4> {
    match listen {
        SocketAddr::V4(addr) => Some(addr),
        SocketAddr::V6(addr) => {
            let ip = if addr.ip().is_unspecified() {
                Ipv4Addr::UNSPECIFIED
            } else {
                addr.ip().to_ipv4_mapped()?
            };
            Some(SocketAddrV4::new(ip, addr.port()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listen_changes_reject_plain_ranges_without_changing_the_address() {
        use std::collections::BTreeMap;

        use crate::tunnel::db::SniRoute;

        let source: crate::tunnel::forward::address::ForwardAddress =
            "203.0.113.4:442".parse().unwrap();
        let target = "10.0.0.2:442".parse().unwrap();
        let original = "0.0.0.0:8443".parse().unwrap();
        let mut db = Model::new(&TunnelDatabase::default()).unwrap();
        db.as_webserver_mut().as_enabled_mut().ser(&true).unwrap();
        for enabled in [false, true] {
            for auto in [false, true] {
                db.as_port_forwards_mut()
                    .ser(&PortForwards(BTreeMap::from([(
                        source.clone(),
                        PortForward::Dnat {
                            target,
                            label: None,
                            enabled,
                            count: 2,
                            auto,
                        },
                    )])))
                    .unwrap();
                update_listen_address(&mut db, original).unwrap();
                for listen in ["0.0.0.0:443", "203.0.113.4:443", "[::]:443"] {
                    if enabled {
                        let error =
                            update_listen_address(&mut db, listen.parse().unwrap()).unwrap_err();
                        assert_eq!(error.kind, ErrorKind::InvalidRequest);
                        assert_eq!(db.as_webserver().as_listen().de().unwrap(), Some(original));
                    } else {
                        update_listen_address(&mut db, listen.parse().unwrap()).unwrap();
                    }
                }
                for listen in ["203.0.113.5:443", "0.0.0.0:444", "[::1]:443"] {
                    update_listen_address(&mut db, listen.parse().unwrap()).unwrap();
                }
            }
        }
        db.as_port_forwards_mut()
            .ser(&PortForwards(BTreeMap::from([(
                "203.0.113.4:443".parse().unwrap(),
                PortForward::Sni {
                    routes: BTreeMap::from([(
                        "service.example.com".to_owned(),
                        SniRoute {
                            target,
                            label: None,
                            enabled: true,
                            auto: false,
                        },
                    )]),
                    fallback: None,
                },
            )])))
            .unwrap();
        update_listen_address(&mut db, "0.0.0.0:443".parse().unwrap()).unwrap();
    }

    #[test]
    fn disabled_ui_allows_configuration_but_checks_conflicts_on_enable() {
        use std::collections::BTreeMap;

        use crate::tunnel::db::SniRoute;

        let source: crate::tunnel::forward::address::ForwardAddress =
            "203.0.113.4:443".parse().unwrap();
        let target = "10.0.0.2:443".parse().unwrap();
        for enabled in [false, true] {
            for forward in [
                PortForward::Dnat {
                    target,
                    label: None,
                    enabled,
                    count: 1,
                    auto: true,
                },
                PortForward::Sni {
                    routes: BTreeMap::from([(
                        "service.example.com".to_owned(),
                        SniRoute {
                            target,
                            label: None,
                            enabled: true,
                            auto: true,
                        },
                    )]),
                    fallback: Some(SniRoute {
                        target,
                        label: None,
                        enabled,
                        auto: true,
                    }),
                },
            ] {
                let mut db = Model::new(&TunnelDatabase::default()).unwrap();
                db.as_port_forwards_mut()
                    .ser(&PortForwards(BTreeMap::from([(source.clone(), forward)])))
                    .unwrap();
                update_listen_address(&mut db, "0.0.0.0:443".parse().unwrap()).unwrap();
                assert!(!db.as_webserver().as_enabled().de().unwrap());
                let result = activate_webserver(&mut db);
                if enabled {
                    assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidRequest);
                    assert!(!db.as_webserver().as_enabled().de().unwrap());
                } else {
                    result.unwrap();
                    assert!(db.as_webserver().as_enabled().de().unwrap());
                }
            }
        }
    }

    #[test]
    fn plain_forwards_respect_enabled_ui_address_and_ranges() {
        let source = "203.0.113.4:443".parse().unwrap();
        let mut web = WebserverInfo {
            enabled: true,
            ..Default::default()
        };
        assert!(!web.reserves_plain_forward(source, 1));
        for listen in [
            "0.0.0.0:443",
            "203.0.113.4:443",
            "[::]:443",
            "[::ffff:203.0.113.4]:443",
        ] {
            web.listen = Some(listen.parse().unwrap());
            web.enabled = false;
            assert!(!web.reserves_plain_forward(source, 1));
            assert!(web.check_plain_forward(source, 1).is_ok());
            web.enabled = true;
            assert!(web.reserves_plain_forward(source, 1));
            assert_eq!(
                web.check_plain_forward(source, 1).unwrap_err().kind,
                ErrorKind::InvalidRequest
            );
            assert!(web.reserves_plain_forward("203.0.113.4:442".parse().unwrap(), 2));
            assert!(!web.reserves_plain_forward("203.0.113.4:442".parse().unwrap(), 1));
            assert!(!web.reserves_plain_forward("203.0.113.4:444".parse().unwrap(), 1));
        }
        for listen in [
            "203.0.113.5:443",
            "127.0.0.1:443",
            "[::1]:443",
            "0.0.0.0:8443",
        ] {
            web.listen = Some(listen.parse().unwrap());
            assert!(!web.reserves_plain_forward(source, 1));
        }
        web.listen = Some("0.0.0.0:65535".parse().unwrap());
        assert!(web.reserves_plain_forward("203.0.113.4:65534".parse().unwrap(), 2));
    }
}

rpc_toolkit::reflect_ts!(WebserverInfo);

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct TunnelCertData {
    pub key: Der64<PKey<Private>>,
    pub cert: Pem<Vec<X509>>,
}

rpc_toolkit::reflect_ts!(TunnelCertData);

#[derive(Clone)]
pub struct TunnelCertHandler {
    pub db: TypedPatchDb<TunnelDatabase>,
    pub crypto_provider: Arc<CryptoProvider>,
}
impl<'a, A> TlsHandler<'a, A> for TunnelCertHandler
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Send + Sync,
{
    async fn get_config(
        &'a mut self,
        _: &'a ClientHello<'a>,
        _: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        let cert_info = self
            .db
            .peek()
            .await
            .as_webserver()
            .as_certificate()
            .de()
            .log_err()??;
        let cert_chain: Vec<_> = cert_info
            .cert
            .0
            .iter()
            .map(|c| Ok::<_, Error>(CertificateDer::from(c.to_der()?)))
            .collect::<Result<_, _>>()
            .log_err()?;
        let cert_key = cert_info.key.0.private_key_to_pkcs8().log_err()?;

        let mut cfg = ServerConfig::builder_with_provider(self.crypto_provider.clone())
            .with_safe_default_protocol_versions()
            .log_err()?
            .with_no_client_auth()
            .with_single_cert(
                cert_chain,
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert_key)),
            )
            .log_err()?;
        cfg.alpn_protocols
            .extend([b"http/1.1".into(), b"h2".into()]);
        Some(TlsHandlerAction::Tls(cfg))
    }
}

pub fn web_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "init",
            from_fn_async_local(init_web)
                .no_display()
                .with_about("about.initialize-webserver"),
        )
        .subcommand(
            "set-listen",
            from_fn_async(set_listen)
                .no_display()
                .with_about("about.set-listen-address-for-webserver")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "get-listen",
            from_fn_async(get_listen)
                .with_display_serializable()
                .with_about("about.get-listen-address-for-webserver")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "get-available-ips",
            from_fn_async(get_available_ips)
                .with_display_serializable()
                .with_about("about.get-available-ip-addresses-to-bind")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "import-certificate",
            from_fn_async(import_certificate_rpc).no_cli(),
        )
        .subcommand(
            "import-certificate",
            from_fn_async_local(import_certificate_cli)
                .no_display()
                .with_about("about.import-certificate-for-webserver"),
        )
        .subcommand(
            "generate-certificate",
            from_fn_async(generate_certificate)
                .with_about("about.generate-certificate-for-webserver")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "get-certificate",
            from_fn_async(get_certificate)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }
                    if let Some(res) = res {
                        println!("{res}");
                    }
                    Ok(())
                })
                .with_about("about.get-certificate-for-webserver")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "enable",
            from_fn_async(enable_web)
                .no_display()
                .with_about("about.enable-webserver")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "disable",
            from_fn_async(disable_web)
                .no_display()
                .with_about("about.disable-webserver")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "uninit",
            from_fn_async(reset_web)
                .no_display()
                .with_about("about.uninitialize-webserver")
                .with_call_remote::<CliContext>(),
        )
}

pub async fn import_certificate_rpc(
    ctx: TunnelContext,
    cert_data: TunnelCertData,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_webserver_mut()
                .as_certificate_mut()
                .ser(&Some(cert_data))
        })
        .await
        .result?;
    Ok(())
}

pub async fn import_certificate_cli(
    HandlerArgs {
        context,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), Error> {
    let mut key_string = String::new();
    let key: Pem<PKey<Private>> =
        prompt_multiline("Please paste in your PEM encoded private key: ", |line| {
            key_string.push_str(&line);
            key_string.push_str("\n");
            if line.trim().starts_with("-----END") {
                return key_string.parse().map(Some).map_err(|e| {
                    key_string.truncate(0);
                    e
                });
            }
            Ok(None)
        })
        .await?;

    let mut chain = Vec::<X509>::new();
    let mut cert_string = String::new();
    prompt_multiline(
        concat!(
            "Please paste in your PEM encoded certificate",
            " (or certificate chain):"
        ),
        |line| {
            cert_string.push_str(&line);
            cert_string.push_str("\n");
            if line.trim().starts_with("-----END") {
                let cert = cert_string.parse::<Pem<X509>>();
                cert_string.truncate(0);
                let cert = cert?;

                let pubkey = cert.0.public_key()?;

                if chain.is_empty() {
                    if !key.public_eq(&pubkey) {
                        return Err(Error::new(
                            eyre!("Certificate does not match key!"),
                            ErrorKind::InvalidSignature,
                        ));
                    }
                }

                if let Some(prev) = chain.last() {
                    if !prev.verify(&pubkey)? {
                        return Err(Error::new(
                            eyre!(concat!(
                                "Invalid Fullchain: ",
                                "Previous cert was not signed by this certificate's key"
                            )),
                            ErrorKind::InvalidSignature,
                        ));
                    }
                }

                // Self-signed check used purely as an end-of-chain sentinel.
                // `X509::verify` returns Err (rather than Ok(false)) when the cert's
                // signature algorithm is incompatible with the candidate pubkey type
                // (e.g. an Ed25519 leaf whose signature is ECDSA from a p256 issuer
                // is verified here against its own Ed25519 pubkey). Treat any such
                // mismatch as "not self-signed" so non-ECDSA leaves don't blow up
                // import with a raw OpenSSL error.
                let is_root = cert.0.verify(&pubkey).unwrap_or(false);

                chain.push(cert.0);

                if is_root { Ok(Some(())) } else { Ok(None) }
            } else {
                Ok(None)
            }
        },
    )
    .await?;

    context
        .call_remote::<TunnelContext>(
            &parent_method.iter().chain(method.iter()).join("."),
            to_value(&TunnelCertData {
                key: Der64(key.0),
                cert: Pem(chain),
            })?,
        )
        .await?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct GenerateCertParams {
    #[arg(help = "help.arg.cert-subject-alt-names")]
    pub subject: Vec<InternedString>,
}

rpc_toolkit::reflect_ts!(GenerateCertParams);

pub async fn generate_certificate(
    ctx: TunnelContext,
    GenerateCertParams { subject }: GenerateCertParams,
) -> Result<Pem<Vec<X509>>, Error> {
    let saninfo = SANInfo::new(&subject.into_iter().collect());

    let branding = CertBranding::start_os("start-tunnel");
    let root_key = crate::net::ssl::gen_nistp256()?;
    let root_cert =
        crate::net::ssl::make_root_cert(&root_key, &branding, root_ca_start_time().await)?;
    let int_key = crate::net::ssl::gen_nistp256()?;
    let int_cert = crate::net::ssl::make_int_cert((&root_key, &root_cert), &int_key, &branding)?;

    let key = crate::net::ssl::gen_nistp256()?;
    let cert = crate::net::ssl::make_leaf_cert((&int_key, &int_cert), (&key, &saninfo), &branding)?;
    let chain = Pem(vec![cert, int_cert, root_cert]);

    ctx.db
        .mutate(|db| {
            db.as_webserver_mut()
                .as_certificate_mut()
                .ser(&Some(TunnelCertData {
                    key: Der64(key),
                    cert: chain.clone(),
                }))
        })
        .await
        .result?;

    Ok(chain)
}

pub async fn get_certificate(ctx: TunnelContext) -> Result<Option<Pem<Vec<X509>>>, Error> {
    ctx.db
        .peek()
        .await
        .as_webserver()
        .as_certificate()
        .de()?
        .map(|cert_data| Ok(cert_data.cert))
        .transpose()
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetListenParams {
    #[arg(help = "help.arg.listen-address")]
    pub listen: SocketAddr,
}

rpc_toolkit::reflect_ts!(SetListenParams);

pub async fn set_listen(
    ctx: TunnelContext,
    SetListenParams { listen }: SetListenParams,
) -> Result<(), Error> {
    let _guard = ctx.forward_write_lock.lock().await;
    listener::UiListener::check_available(listen).with_ctx(|_| {
        (
            ErrorKind::Network,
            format!("{} is not available to bind to", listen),
        )
    })?;

    ctx.db
        .mutate(|db| update_listen_address(db, listen))
        .await
        .result
}

fn update_listen_address(db: &mut Model<TunnelDatabase>, listen: SocketAddr) -> Result<(), Error> {
    let mut web = db.as_webserver().de()?;
    web.listen = Some(listen);
    web.check_forward_conflicts(&db.as_port_forwards().de()?)?;
    db.as_webserver_mut().as_listen_mut().ser(&Some(listen))
}

fn activate_webserver(db: &mut Model<TunnelDatabase>) -> Result<(), Error> {
    let mut web = db.as_webserver().de()?;
    web.enabled = true;
    web.check_forward_conflicts(&db.as_port_forwards().de()?)?;
    db.as_webserver_mut().as_enabled_mut().ser(&true)
}

pub async fn get_listen(ctx: TunnelContext) -> Result<Option<SocketAddr>, Error> {
    ctx.db.peek().await.as_webserver().as_listen().de()
}

pub async fn get_available_ips(ctx: TunnelContext) -> Result<Vec<IpAddr>, Error> {
    let ips = ctx.net_iface.peek(|interfaces| {
        interfaces
            .values()
            .flat_map(|info| {
                info.ip_info
                    .iter()
                    .flat_map(|ip_info| ip_info.subnets.iter().map(|subnet| subnet.addr()))
            })
            .collect::<Vec<IpAddr>>()
    });

    Ok(ips)
}

pub async fn enable_web(ctx: TunnelContext) -> Result<(), Error> {
    let _guard = ctx.forward_write_lock.lock().await;
    ctx.db
        .mutate(|db| {
            if db.as_webserver().as_listen().transpose_ref().is_none() {
                return Err(Error::new(
                    eyre!("Listen is not set"),
                    ErrorKind::ParseNetAddress,
                ));
            }
            if db.as_webserver().as_certificate().transpose_ref().is_none() {
                return Err(Error::new(
                    eyre!("Certificate is not set"),
                    ErrorKind::OpenSsl,
                ));
            }
            if db.as_password().transpose_ref().is_none() {
                return Err(Error::new(
                    eyre!("Password is not set"),
                    ErrorKind::Authorization,
                ));
            };
            activate_webserver(db)
        })
        .await
        .result
}

pub async fn disable_web(ctx: TunnelContext) -> Result<(), Error> {
    ctx.db
        .mutate(|db| db.as_webserver_mut().as_enabled_mut().ser(&false))
        .await
        .result
}

pub async fn reset_web(ctx: TunnelContext) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_webserver_mut().as_enabled_mut().ser(&false)?;
            db.as_webserver_mut().as_listen_mut().ser(&None)?;
            db.as_webserver_mut().as_certificate_mut().ser(&None)?;
            db.as_password_mut().ser(&None)?;
            Ok(())
        })
        .await
        .result
}

fn is_valid_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }

    let labels: Vec<&str> = domain.split('.').collect();

    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }

        if label.chars().next().map_or(true, |c| c == '-')
            || label.chars().next_back().map_or(true, |c| c == '-')
        {
            return false;
        }
    }

    true
}

async fn web_is_initialized_disabled(ctx: &CliContext) -> Result<bool, Error> {
    let enabled = from_value::<Dump>(
        ctx.call_remote::<TunnelContext>("db.dump", json!({ "pointer": "/webserver/enabled" }))
            .await?,
    )?;
    if from_value::<bool>(enabled.value)? {
        return Ok(false);
    }

    let listen = from_value::<Option<SocketAddr>>(
        ctx.call_remote::<TunnelContext>("web.get-listen", json!({}))
            .await?,
    )?;
    if listen.is_none() {
        return Ok(false);
    }

    if ctx
        .call_remote::<TunnelContext>("web.get-certificate", json!({}))
        .await?
        .is_null()
    {
        return Ok(false);
    }

    let password = from_value::<Dump>(
        ctx.call_remote::<TunnelContext>("db.dump", json!({ "pointer": "/password" }))
            .await?,
    )?;
    Ok(!password.value.is_null())
}

pub async fn init_web(ctx: CliContext) -> Result<(), Error> {
    if web_is_initialized_disabled(&ctx).await? {
        println!("{}", t!("tunnel.web.already-initialized-disabled"));
        return Ok(());
    }

    let mut password = None;
    let mut cert_generated = false;
    loop {
        match ctx
            .call_remote::<TunnelContext>("web.enable", json!({}))
            .await
        {
            Ok(_) => {
                let listen = from_value::<SocketAddr>(
                    ctx.call_remote::<TunnelContext>("web.get-listen", json!({}))
                        .await?,
                )?;

                println!("✅ Success! ✅");
                print!("StartTunnel installed successfully. Below is your Web URL");
                match (password.is_some(), cert_generated) {
                    (true, true) => {
                        print!(", password, and Root Certificate Authority (Root CA)")
                    }
                    (true, false) => print!(" and password"),
                    (false, true) => print!(" and Root Certificate Authority (Root CA)"),
                    (false, false) => {}
                }
                println!(".");
                println!();
                println!("🌐 Web URL");
                println!("https://{listen}");
                if listen.ip().is_unspecified() {
                    println!(concat!(
                        "Note: this is the unspecified address. ",
                        "This means you can use any IP address available to this device to connect. ",
                        "Using the above address as-is will only work from this device."
                    ));
                } else if listen.ip().is_loopback() {
                    println!(concat!(
                        "Note: this is a loopback address. ",
                        "This is only recommended if you are planning to run a proxy in front of the web ui. ",
                        "Using the above address as-is will only work from this device."
                    ));
                }
                println!();

                if let Some(password) = password {
                    println!("🔒 Password");
                    println!("{password}");
                    println!();
                    println!(concat!(
                        "If you lose or forget your password, you can reset it using the following command: ",
                        "start-tunnel auth reset-password"
                    ));
                } else {
                    println!(concat!(
                        "Your password was set up previously. ",
                        "If you don't remember it, you can reset it using the command: ",
                        "start-tunnel auth reset-password"
                    ));
                }
                println!();

                if cert_generated {
                    let cert = from_value::<Pem<Vec<X509>>>(
                        ctx.call_remote::<TunnelContext>("web.get-certificate", json!({}))
                            .await?,
                    )?
                    .0
                    .pop()
                    .map(Pem)
                    .or_not_found("certificate in chain")?;
                    println!("📝 Root CA:");
                    print!("{cert}\n");
                    println!(
                        "Follow instructions to trust your Root CA (recommended): https://docs.start9.com/start-tunnel/installing.html#certificate-options"
                    );
                }

                return Ok(());
            }
            Err(e) if e.kind == ErrorKind::ParseNetAddress => {
                println!("Select the IP address at which to host the web interface:");

                let mut suggested_addrs = from_value::<Vec<IpAddr>>(
                    ctx.call_remote::<TunnelContext>("web.get-available-ips", json!({}))
                        .await?,
                )?;

                suggested_addrs.retain(|ip| match ip {
                    IpAddr::V4(a) => !a.is_loopback() && !a.is_private(),
                    IpAddr::V6(a) => !a.is_loopback() && !a.is_unicast_link_local(),
                });

                let ip = if suggested_addrs.len() == 1 {
                    suggested_addrs[0]
                } else if suggested_addrs.is_empty() {
                    prompt("Listen Address: ", parse_as::<IpAddr>("IP Address"), None).await?
                } else if suggested_addrs.len() > 16 {
                    prompt(
                        &format!("Listen Address [{}]: ", suggested_addrs[0]),
                        parse_as::<IpAddr>("IP Address"),
                        Some(suggested_addrs[0]),
                    )
                    .await?
                } else {
                    *choose("Listen Address:", &suggested_addrs).await?
                };

                println!(concat!(
                    "Enter the port at which to host the web interface. ",
                    "The recommended default is 8443. ",
                    "If you change the default, choose an uncommon port to avoid conflicts: "
                ));
                let port = prompt("Port [8443]: ", parse_as::<u16>("port"), Some(8443)).await?;

                let listen = SocketAddr::new(ip, port);

                ctx.call_remote::<TunnelContext>(
                    "web.set-listen",
                    to_value(&SetListenParams { listen })?,
                )
                .await?;

                println!();
            }
            Err(e) if e.kind == ErrorKind::OpenSsl => {
                enum Choice {
                    Generate,
                    Provide,
                }
                impl std::fmt::Display for Choice {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        match self {
                            Self::Generate => write!(f, "Generate"),
                            Self::Provide => write!(f, "Provide"),
                        }
                    }
                }
                let options = vec![Choice::Generate, Choice::Provide];
                println!(concat!(
                    "An SSL certificate is required. ",
                    "You can have StartTunnel generate a certificate signed by its own Root CA, ",
                    "or provide your own certificate (and key)."
                ));
                let choice = choose("SSL Certificate:", &options).await?;

                match choice {
                    Choice::Generate => {
                        let listen = from_value::<Option<SocketAddr>>(
                            ctx.call_remote::<TunnelContext>("web.get-listen", json!({}))
                                .await?,
                        )?
                        .filter(|a| !a.ip().is_unspecified());

                        let san_info = if let Some(listen) = listen {
                            vec![InternedString::from_display(&listen.ip())]
                        } else {
                            println!(
                                "List all IP addresses and domains for which to sign the certificate, separated by commas."
                            );
                            prompt(
                                "Subject Alternative Name(s): ",
                                |s| {
                                    s.split(",")
                                        .map(|s| {
                                            let s = s.trim();
                                            if let Ok(ip) = s.parse::<IpAddr>() {
                                                Ok(InternedString::from_display(&ip))
                                            } else if is_valid_domain(s) {
                                                Ok(s.into())
                                            } else {
                                                Err(format!(
                                                    "{s} is not a valid ip address or domain"
                                                ))
                                            }
                                        })
                                        .collect()
                                },
                                listen.map(|l| vec![InternedString::from_display(&l.ip())]),
                            )
                            .await?
                        };

                        ctx.call_remote::<TunnelContext>(
                            "web.generate-certificate",
                            to_value(&GenerateCertParams { subject: san_info })?,
                        )
                        .await?;
                        cert_generated = true;
                    }
                    Choice::Provide => {
                        import_certificate_cli(HandlerArgs {
                            context: ctx.clone(),
                            parent_method: vec!["web", "import-certificate"].into(),
                            method: VecDeque::new(),
                            params: Empty {},
                            inherited_params: Empty {},
                            raw_params: json!({}),
                            rpc_stream: None,
                        })
                        .await?;
                    }
                }

                println!();
            }
            Err(e) if e.kind == ErrorKind::Authorization => {
                println!("Generating a random password...");
                let params = SetPasswordParams {
                    password: base32::encode(
                        base32::Alphabet::Rfc4648Lower { padding: false },
                        &rand::random::<[u8; 16]>(),
                    ),
                };
                ctx.call_remote::<TunnelContext>("auth.set-password", to_value(&params)?)
                    .await?;

                password = Some(params.password);

                println!();
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod init_tests {
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use rpc_toolkit::{HandlerExt, ParentHandler, Server, from_fn_async};
    use tower_service::Service;

    use super::*;
    use crate::context::config::ClientConfig;
    use crate::middleware::auth::session::RpcSessionUpgrader;
    use crate::registry::context::{RegistryContext, test_context};
    use crate::util::future::NonDetachingJoinHandle;

    struct StubState {
        enabled: bool,
        listen: Option<SocketAddr>,
        certificate: Option<String>,
        password: Option<String>,
        enable_calls: usize,
        set_password_calls: usize,
    }

    impl StubState {
        fn initialized(enabled: bool) -> Self {
            Self {
                enabled,
                listen: Some("127.0.0.1:8443".parse().unwrap()),
                certificate: Some(
                    "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----\n".into(),
                ),
                password: Some("$argon2id$stub".into()),
                enable_calls: 0,
                set_password_calls: 0,
            }
        }
    }

    struct StubServer {
        addr: SocketAddr,
        state: Arc<Mutex<StubState>>,
        ca_path: PathBuf,
        id_key_path: PathBuf,
        _temp: tempfile::TempDir,
        _handle: NonDetachingJoinHandle<()>,
    }

    fn test_signing_key() -> crate::sign::AnySigningKey {
        crate::sign::AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[7; 32]))
    }

    fn build_cli_context(addr: SocketAddr, ca_path: PathBuf, id_key_path: PathBuf) -> CliContext {
        let ctx = CliContext::init(ClientConfig {
            id_key_path: Some(id_key_path),
            root_ca: Some(vec![ca_path]),
            tunnel: Some(addr),
            ..Default::default()
        })
        .unwrap();
        ctx.id_key
            .set(crate::sign::Signer::Key(test_signing_key()))
            .unwrap();
        ctx
    }

    async fn spawn_stub_server(initial: StubState) -> StubServer {
        let temp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(initial));
        let context = test_context(temp.path().into(), test_signing_key().verifying_key()).await;

        let dump_state = state.clone();
        let listen_state = state.clone();
        let cert_state = state.clone();
        let enable_state = state.clone();
        let pw_state = state.clone();

        let handler = ParentHandler::<RegistryContext>::new()
            .subcommand(
                "db",
                ParentHandler::<RegistryContext>::new().subcommand(
                    "dump",
                    from_fn_async(move |_: RegistryContext, params: Value| {
                        let dump_state = dump_state.clone();
                        async move {
                            let pointer = params
                                .get("pointer")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    Error::new(eyre!("missing pointer"), ErrorKind::InvalidRequest)
                                })?;
                            let guard = dump_state.lock().unwrap();
                            let value = match pointer {
                                "/webserver/enabled" => Value::Bool(guard.enabled),
                                "/password" => to_value(&guard.password)?,
                                other => {
                                    return Err(Error::new(
                                        eyre!("unexpected pointer: {other}"),
                                        ErrorKind::InvalidRequest,
                                    ));
                                }
                            };
                            Ok::<_, Error>(json!({ "id": 1, "value": value }))
                        }
                    })
                    .no_cli()
                    .no_ts(),
                ),
            )
            .subcommand(
                "web",
                ParentHandler::<RegistryContext>::new()
                    .subcommand(
                        "get-listen",
                        from_fn_async(move |_: RegistryContext| {
                            let listen_state = listen_state.clone();
                            async move {
                                let listen = listen_state.lock().unwrap().listen;
                                Ok::<_, Error>(listen)
                            }
                        })
                        .no_cli()
                        .no_ts(),
                    )
                    .subcommand(
                        "get-certificate",
                        from_fn_async(move |_: RegistryContext| {
                            let cert_state = cert_state.clone();
                            async move {
                                let cert = cert_state.lock().unwrap().certificate.clone();
                                Ok::<_, Error>(cert)
                            }
                        })
                        .no_cli()
                        .no_ts(),
                    )
                    .subcommand(
                        "enable",
                        from_fn_async(move |_: RegistryContext| {
                            let enable_state = enable_state.clone();
                            async move {
                                let mut guard = enable_state.lock().unwrap();
                                guard.enable_calls += 1;
                                if guard.listen.is_none() {
                                    return Err(Error::new(
                                        eyre!("Listen is not set"),
                                        ErrorKind::ParseNetAddress,
                                    ));
                                }
                                if guard.certificate.is_none() {
                                    return Err(Error::new(
                                        eyre!("Certificate is not set"),
                                        ErrorKind::OpenSsl,
                                    ));
                                }
                                if guard.password.is_none() {
                                    return Err(Error::new(
                                        eyre!("Password is not set"),
                                        ErrorKind::Authorization,
                                    ));
                                }
                                guard.enabled = true;
                                Ok::<_, Error>(())
                            }
                        })
                        .no_cli()
                        .no_ts(),
                    ),
            )
            .subcommand(
                "auth",
                ParentHandler::<RegistryContext>::new().subcommand(
                    "set-password",
                    from_fn_async(move |_: RegistryContext, params: SetPasswordParams| {
                        let pw_state = pw_state.clone();
                        async move {
                            let mut guard = pw_state.lock().unwrap();
                            guard.set_password_calls += 1;
                            guard.password = Some(params.password);
                            Ok::<_, Error>(())
                        }
                    })
                    .no_cli()
                    .no_ts(),
                ),
            );

        let server = Server::new(
            move || {
                let context = context.clone();
                async move { Ok(context) }
            },
            handler,
        )
        .for_http();
        let app =
            axum::Router::new().route(RpcSessionUpgrader::PATH, RpcSessionUpgrader::route(server));

        let (key, cert) = crate::net::tls::test::self_signed_for_loopback();
        let mut tls_config = crate::net::tls::test::server_config(&key, &cert);
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let ca_path = temp.path().join("ca.pem");
        std::fs::write(&ca_path, cert.to_pem().unwrap()).unwrap();
        let id_key_path = temp.path().join("id.pem");

        let handle = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let acceptor = acceptor.clone();
                let app = app.clone();
                connections.spawn(async move {
                    let Ok(socket) = acceptor.accept(socket).await else {
                        return;
                    };
                    let service = hyper::service::service_fn(
                        move |request: http::Request<hyper::body::Incoming>| {
                            let mut app = app.clone();
                            async move { app.call(request.map(Body::new)).await }
                        },
                    );
                    let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                        .serve_connection_with_upgrades(TokioIo::new(socket), service)
                        .await;
                });
            }
        })
        .into();

        StubServer {
            addr,
            state,
            ca_path,
            id_key_path,
            _temp: temp,
            _handle: handle,
        }
    }

    async fn run_init_web_subprocess(test_name: &str, stub: &StubServer) -> std::process::Output {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env("START_CORE_TEST_TUNNEL_INIT_ADDR", stub.addr.to_string())
                .env("START_CORE_TEST_TUNNEL_INIT_CA", &stub.ca_path)
                .env("START_CORE_TEST_TUNNEL_INIT_ID_KEY", &stub.id_key_path)
                .env("LANG", "en_US.UTF-8")
                .output(),
        )
        .await
        .unwrap()
        .unwrap()
    }

    async fn maybe_run_subprocess_child() -> bool {
        let Ok(addr) = std::env::var("START_CORE_TEST_TUNNEL_INIT_ADDR") else {
            return false;
        };
        let ca_path = PathBuf::from(std::env::var("START_CORE_TEST_TUNNEL_INIT_CA").unwrap());
        let id_key_path =
            PathBuf::from(std::env::var("START_CORE_TEST_TUNNEL_INIT_ID_KEY").unwrap());
        let ctx = build_cli_context(addr.parse().unwrap(), ca_path, id_key_path);
        init_web(ctx).await.unwrap();
        true
    }

    #[tokio::test]
    async fn init_web_keeps_initialized_disabled_ui_off() {
        if maybe_run_subprocess_child().await {
            return;
        }
        let stub = spawn_stub_server(StubState::initialized(false)).await;
        let output = run_init_web_subprocess(
            "tunnel::web::init_tests::init_web_keeps_initialized_disabled_ui_off",
            &stub,
        )
        .await;
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
        let state = stub.state.lock().unwrap();
        assert_eq!(state.enable_calls, 0);
        assert!(!state.enabled);
        assert!(
            stdout
                .contains(t!("tunnel.web.already-initialized-disabled", locale = "en_US").as_ref()),
            "{stdout}"
        );
        assert!(!stdout.contains("Success"), "{stdout}");
        assert!(!stdout.contains("https://"), "{stdout}");
    }

    #[tokio::test]
    async fn init_web_preserves_already_enabled_ui_behavior() {
        if maybe_run_subprocess_child().await {
            return;
        }
        let stub = spawn_stub_server(StubState::initialized(true)).await;
        let output = run_init_web_subprocess(
            "tunnel::web::init_tests::init_web_preserves_already_enabled_ui_behavior",
            &stub,
        )
        .await;
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
        let state = stub.state.lock().unwrap();
        assert_eq!(state.enable_calls, 1);
        assert!(state.enabled);
        assert!(stdout.contains("https://127.0.0.1:8443"), "{stdout}");
    }

    #[tokio::test]
    async fn init_web_completes_partial_setup_when_disabled_and_missing_password() {
        let mut initial = StubState::initialized(false);
        initial.password = None;
        let stub = spawn_stub_server(initial).await;
        let ctx = build_cli_context(stub.addr, stub.ca_path.clone(), stub.id_key_path.clone());

        init_web(ctx).await.unwrap();

        let state = stub.state.lock().unwrap();
        assert_eq!(state.enable_calls, 2);
        assert_eq!(state.set_password_calls, 1);
        assert!(state.enabled);
        assert!(state.password.is_some());
    }

    #[tokio::test]
    async fn web_is_initialized_disabled_matrix() {
        let cases = [
            ("enabled_all_set", StubState::initialized(true), false),
            (
                "fresh_uninitialized",
                StubState {
                    listen: None,
                    certificate: None,
                    password: None,
                    ..StubState::initialized(false)
                },
                false,
            ),
            (
                "missing_listen",
                StubState {
                    listen: None,
                    ..StubState::initialized(false)
                },
                false,
            ),
            (
                "missing_certificate",
                StubState {
                    certificate: None,
                    ..StubState::initialized(false)
                },
                false,
            ),
            (
                "missing_password",
                StubState {
                    password: None,
                    ..StubState::initialized(false)
                },
                false,
            ),
            ("disabled_all_set", StubState::initialized(false), true),
        ];

        for (label, initial, expected) in cases {
            let stub = spawn_stub_server(initial).await;
            let ctx = build_cli_context(stub.addr, stub.ca_path.clone(), stub.id_key_path.clone());
            assert_eq!(
                web_is_initialized_disabled(&ctx).await.unwrap(),
                expected,
                "{label}"
            );
        }
    }
}
