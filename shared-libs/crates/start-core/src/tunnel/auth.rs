use std::collections::BTreeSet;

use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use patch_db::HasModel;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthKeys, LoginContext};
use crate::context::CliContext;
use crate::middleware::auth::DbContext;
use crate::middleware::auth::local::LocalAuthContext;
use crate::middleware::auth::signature::{
    LoginMetadata, SignatureAuthContext, SignatureAuthorizationContext, check_enrolled,
    url_host_str,
};
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::TunnelDatabase;

impl DbContext for TunnelContext {
    type Database = TunnelDatabase;
    fn db(&self) -> &TypedPatchDb<Self::Database> {
        &self.db
    }
}
impl SignatureAuthContext for TunnelContext {
    fn server_identity(&self) -> Result<crate::sign::session::ServerIdentity, Error> {
        TunnelContext::server_identity(self).map(Into::into)
    }
    fn replay_check(&self) -> crate::middleware::auth::signature::RequestReplayCheck {
        TunnelContext::replay_check(self)
    }
    fn with_secure_session(self, session: crate::middleware::auth::session::SecureSession) -> Self {
        TunnelContext::with_secure_session(self, session)
    }
    fn mutate_nonce_cache<
        F: FnOnce(&mut crate::middleware::auth::signature::NonceCache) -> T,
        T,
    >(
        &self,
        f: F,
    ) -> T {
        self.auth_sig_nonce_cache.mutate(f)
    }
    /// Trusts the Tunnel host clock.
    async fn clock_synced(&self) -> bool {
        true
    }
    async fn sig_context(
        &self,
        _extensions: &http::Extensions,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        let peek = self.db().peek().await;
        peek.as_webserver()
            .as_listen()
            .de()
            .map(|a| a.as_ref().map(|a| url_host_str(a.ip())))
            .transpose()
            .into_iter()
            .chain(
                std::iter::once_with(move || {
                    peek.as_webserver()
                        .as_certificate()
                        .de()
                        .ok()
                        .flatten()
                        .and_then(|cert_data| cert_data.cert.0.first().cloned())
                        .and_then(|cert| cert.subject_alt_names())
                        .into_iter()
                        .flatten()
                        .filter_map(|san| {
                            san.dnsname().map(InternedString::from).or_else(|| {
                                san.ipaddress().and_then(|ip_bytes| {
                                    let ip: std::net::IpAddr = match ip_bytes.len() {
                                        4 => std::net::IpAddr::V4(std::net::Ipv4Addr::from(
                                            <[u8; 4]>::try_from(ip_bytes).ok()?,
                                        )),
                                        16 => std::net::IpAddr::V6(std::net::Ipv6Addr::from(
                                            <[u8; 16]>::try_from(ip_bytes).ok()?,
                                        )),
                                        _ => return None,
                                    };
                                    Some(url_host_str(ip))
                                })
                            })
                        })
                        .map(Ok)
                        .collect::<Vec<_>>()
                })
                .flatten(),
            )
    }
}
impl SignatureAuthorizationContext for TunnelContext {
    type AdditionalMetadata = LoginMetadata;
    type CheckPubkeyRes = Option<InternedString>;
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
    fn remove_enrolled_keys(
        db: &mut Model<Self::Database>,
        keys: &BTreeSet<InternedString>,
    ) -> Result<(), Error> {
        let auth_keys = db.as_session_pubkeys_mut();
        for key in keys {
            auth_keys.remove(key)?;
        }
        Ok(())
    }
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&crate::sign::AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
        check_enrolled(pubkey, metadata.login, |key| {
            self.auth_key_is_enrolled(db, key)
        })
    }
    async fn post_auth_hook(
        &self,
        key: Self::CheckPubkeyRes,
        _: &rpc_toolkit::RpcRequest,
    ) -> Result<(), Error> {
        if let Some(key) = key {
            crate::auth::touch_last_active(self, &key).await?;
        }
        Ok(())
    }
}
impl LocalAuthContext for TunnelContext {
    const LOCAL_AUTH_COOKIE_PATH: &str = "/run/startos/tunnel.authcookie";
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str = "root:root";
}
impl LoginContext for TunnelContext {
    fn auth_key_is_enrolled(
        &self,
        db: &Model<Self::Database>,
        key: &InternedString,
    ) -> Result<bool, Error> {
        Ok(Self::access_auth_keys(&mut db.clone())
            .de()?
            .0
            .contains_key(&**key))
    }
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys> {
        db.as_session_pubkeys_mut()
    }
    fn password_hash(db: &Model<Self::Database>) -> Result<Option<String>, Error> {
        db.as_password().de()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct SignerInfo {
    pub name: InternedString,
}

rpc_toolkit::reflect_ts!(SignerInfo);

pub fn auth_api<C: Context>() -> ParentHandler<C> {
    crate::auth::auth::<C, TunnelContext, CliContext>()
        .subcommand("set-password", from_fn_async(set_password_rpc).no_cli())
        .subcommand(
            "set-password",
            from_fn_async(set_password_cli)
                .no_display()
                .with_about("about.set-user-interface-password"),
        )
        .subcommand(
            "reset-password",
            from_fn_async(reset_password)
                .no_display()
                .with_about("about.reset-user-interface-password"),
        )
}

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
pub struct SetPasswordParams {
    pub password: String,
}

rpc_toolkit::reflect_ts!(SetPasswordParams);
rpc_toolkit::ts_export!(SetPasswordParams, namespaces = ["tunnel"]);

pub async fn set_password_rpc(
    ctx: TunnelContext,
    SetPasswordParams { password }: SetPasswordParams,
) -> Result<(), Error> {
    let pwhash = argon2::hash_encoded(
        password.as_bytes(),
        &rand::random::<[u8; 16]>(),
        &argon2::Config::rfc9106_low_mem(),
    )
    .with_kind(ErrorKind::PasswordHashGeneration)?;
    ctx.db
        .mutate(|db| db.as_password_mut().ser(&Some(pwhash)))
        .await
        .result?;

    Ok(())
}

pub async fn set_password_cli(
    HandlerArgs {
        context,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), Error> {
    let password = rpassword::prompt_password("New Password: ")?;
    let confirm = rpassword::prompt_password("Confirm Password: ")?;

    if password != confirm {
        return Err(Error::new(
            eyre!("Passwords do not match"),
            ErrorKind::InvalidRequest,
        ));
    }

    context
        .call_remote::<TunnelContext>(
            &parent_method.iter().chain(method.iter()).join("."),
            to_value(&SetPasswordParams { password })?,
        )
        .await?;

    println!("Password set successfully");

    Ok(())
}

pub async fn reset_password(ctx: CliContext) -> Result<(), Error> {
    println!("Generating a random password...");
    let params = SetPasswordParams {
        password: base32::encode(
            base32::Alphabet::Rfc4648Lower { padding: false },
            &rand::random::<[u8; 16]>(),
        ),
    };

    ctx.call_remote::<TunnelContext>("auth.set-password", to_value(&params)?)
        .await?;

    println!("Your new password is:");
    println!("{}", params.password);

    Ok(())
}
