use std::collections::BTreeSet;

use chrono::Utc;
use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use patch_db::HasModel;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::{AuthKeys, LoginContext, Session, check_password};
use crate::context::CliContext;
use crate::middleware::auth::DbContext;
use crate::middleware::auth::local::LocalAuthContext;
use crate::middleware::auth::signature::{
    LoginMetadata, SignatureAuthContext, check_enrolled, url_host_str,
};
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::AnyVerifyingKey;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::TunnelDatabase;
use crate::util::serde::{HandlerExtSerde, display_serializable};

impl DbContext for TunnelContext {
    type Database = TunnelDatabase;
    fn db(&self) -> &TypedPatchDb<Self::Database> {
        &self.db
    }
}
impl SignatureAuthContext for TunnelContext {
    type AdditionalMetadata = LoginMetadata;
    type CheckPubkeyRes = Option<InternedString>;
    fn mutate_nonce_cache<
        F: FnOnce(&mut crate::middleware::auth::signature::NonceCache) -> T,
        T,
    >(
        &self,
        f: F,
    ) -> T {
        self.auth_sig_nonce_cache.mutate(f)
    }
    async fn clock_synced(&self) -> bool {
        true // Assume. Validating VPS clock sync out of scope for now,
    }
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
    async fn sig_context(
        &self,
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
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&crate::sign::AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
        check_enrolled(pubkey, metadata.login, |key| {
            Ok(db.as_session_pubkeys().de()?.0.contains_key(&**key))
        })
    }
    async fn post_auth_hook(
        &self,
        key: Self::CheckPubkeyRes,
        _: &rpc_toolkit::RpcRequest,
    ) -> Result<(), Error> {
        if let Some(key) = key {
            self.db
                .mutate(|db| {
                    db.as_session_pubkeys_mut().mutate(|keys| {
                        if let Some(info) = keys.0.get_mut(&*key) {
                            info.last_active = Utc::now();
                        }
                        Ok(())
                    })
                })
                .await
                .result?;
        }
        Ok(())
    }
}
impl LocalAuthContext for TunnelContext {
    const LOCAL_AUTH_COOKIE_PATH: &str = "/run/startos/tunnel.authcookie";
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str = "root:root";
}
impl LoginContext for TunnelContext {
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys> {
        db.as_session_pubkeys_mut()
    }
    fn check_password(db: &Model<Self::Database>, password: &str) -> Result<(), Error> {
        check_password(&db.as_password().de()?.unwrap_or_default(), password)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, TS, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct SignerInfo {
    pub name: InternedString,
}

pub fn auth_api<C: Context>() -> ParentHandler<C> {
    crate::auth::auth::<C, TunnelContext>()
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
        .subcommand(
            "key",
            ParentHandler::<C>::new()
                .subcommand(
                    "add",
                    from_fn_async(add_key)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.add-new-authorized-key")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "remove",
                    from_fn_async(remove_key)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.remove-authorized-key")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "list",
                    from_fn_async(list_keys)
                        .with_metadata("sync_db", Value::Bool(true))
                        .with_display_serializable()
                        .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                            use prettytable::*;
                            if let Some(format) = params.format {
                                return display_serializable(format, res);
                            }
                            let mut table = Table::new();
                            table.add_row(row![bc => "NAME", "KEY"]);
                            for (key, info) in res.0 {
                                table.add_row(row![info.name.as_deref().unwrap_or("-"), key]);
                            }
                            table.print_tty(false)?;
                            Ok(())
                        })
                        .with_about("about.list-authorized-keys")
                        .with_call_remote::<CliContext>(),
                )
                .with_about("about.commands-authorized-keys"),
        )
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct AddKeyParams {
    pub name: InternedString,
    pub key: AnyVerifyingKey,
}

pub async fn add_key(
    ctx: TunnelContext,
    AddKeyParams { name, key }: AddKeyParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_session_pubkeys_mut().mutate(|session_pubkeys| {
                session_pubkeys.0.insert(
                    key.interned_pem(),
                    Session {
                        name: Some(name),
                        ..Default::default()
                    },
                );
                Ok(())
            })
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemoveKeyParams {
    pub key: AnyVerifyingKey,
}

pub async fn remove_key(
    ctx: TunnelContext,
    RemoveKeyParams { key }: RemoveKeyParams,
) -> Result<(), Error> {
    ctx.unenroll([key.interned_pem()]).await?;
    Ok(())
}

pub async fn list_keys(ctx: TunnelContext) -> Result<AuthKeys, Error> {
    ctx.db.peek().await.into_session_pubkeys().de()
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct SetPasswordParams {
    pub password: String,
}

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
