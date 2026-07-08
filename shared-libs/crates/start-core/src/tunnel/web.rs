use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use clap::Parser;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use rpc_toolkit::{
    Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async, from_fn_async_local,
};
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::ClientHello;
use ts_rs::TS;

use crate::context::CliContext;
use crate::net::ssl::{CertBranding, SANInfo, root_ca_start_time};
use crate::net::tls::{TlsHandler, TlsHandlerAction};
use crate::net::web_server::Accept;
use crate::prelude::*;
use crate::tunnel::auth::SetPasswordParams;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::TunnelDatabase;
use crate::util::serde::{HandlerExtSerde, Pem, display_serializable};
use crate::util::tui::{choose, parse_as, prompt, prompt_multiline};

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct WebserverInfo {
    pub enabled: bool,
    pub listen: Option<SocketAddr>,
    pub certificate: Option<TunnelCertData>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TunnelCertData {
    pub key: Pem<PKey<Private>>,
    pub cert: Pem<Vec<X509>>,
}

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
                key,
                cert: Pem(chain),
            })?,
        )
        .await?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct GenerateCertParams {
    #[arg(help = "help.arg.cert-subject-alt-names")]
    pub subject: Vec<InternedString>,
}

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
                    key: Pem(key),
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

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetListenParams {
    #[arg(help = "help.arg.listen-address")]
    pub listen: SocketAddr,
}

pub async fn set_listen(
    ctx: TunnelContext,
    SetListenParams { listen }: SetListenParams,
) -> Result<(), Error> {
    // Validate that the address is available to bind
    tokio::net::TcpListener::bind(listen)
        .await
        .with_kind(ErrorKind::Network)
        .with_ctx(|_| {
            (
                ErrorKind::Network,
                format!("{} is not available to bind to", listen),
            )
        })?;

    ctx.db
        .mutate(|db| {
            db.as_webserver_mut().as_listen_mut().ser(&Some(listen))?;

            Ok(())
        })
        .await
        .result
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
            db.as_webserver_mut().as_enabled_mut().ser(&true)
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

pub async fn init_web(ctx: CliContext) -> Result<(), Error> {
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
