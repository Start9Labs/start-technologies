use clap::Parser;
use imbl_value::InternedString;
use lazy_format::lazy_format;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;
use ts_rs::TS;

use crate::context::RpcContext;
use crate::db::model::public::{RestartReason, ServerInfo};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{copy_file, write_file_atomic};

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize, ts_rs::TS)]
#[ts(type = "string")]
pub struct ServerHostname(InternedString);
impl std::ops::Deref for ServerHostname {
    type Target = InternedString;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<str> for ServerHostname {
    fn as_ref(&self) -> &str {
        &***self
    }
}

impl ServerHostname {
    fn validate(&self) -> Result<(), Error> {
        if self.0.is_empty() {
            return Err(Error::new(
                eyre!("{}", t!("hostname.empty")),
                ErrorKind::InvalidRequest,
            ));
        }
        if let Some(c) = self
            .0
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || c == &'-') || c.is_ascii_uppercase())
        {
            return Err(Error::new(
                eyre!("{}", t!("hostname.invalid-character", char = c)),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok(())
    }

    pub fn new(hostname: InternedString) -> Result<Self, Error> {
        let res = Self(hostname);
        res.validate()?;
        Ok(res)
    }

    pub fn lan_address(&self) -> InternedString {
        InternedString::from_display(&lazy_format!("https://{}.local", self.0))
    }

    pub fn local_domain_name(&self) -> InternedString {
        InternedString::from_display(&lazy_format!("{}.local", self.0))
    }

    pub fn load(server_info: &Model<ServerInfo>) -> Result<Self, Error> {
        Ok(Self(server_info.as_hostname().de()?))
    }

    pub fn save(&self, server_info: &mut Model<ServerInfo>) -> Result<(), Error> {
        server_info.as_hostname_mut().ser(&**self)
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize, ts_rs::TS)]
#[ts(type = "string")]
pub struct ServerHostnameInfo {
    pub name: InternedString,
    pub hostname: ServerHostname,
}

lazy_static::lazy_static! {
    static ref ADJECTIVES: Vec<String> = include_str!("./assets/adjectives.txt").lines().map(|x| x.to_string()).collect();
    static ref NOUNS: Vec<String> = include_str!("./assets/nouns.txt").lines().map(|x| x.to_string()).collect();
}
impl AsRef<str> for ServerHostnameInfo {
    fn as_ref(&self) -> &str {
        &self.hostname
    }
}

fn normalize(s: &str) -> InternedString {
    let mut prev_was_dash = true;
    let mut normalized = s
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                prev_was_dash = false;
                Some(c.to_ascii_lowercase())
            } else if (c == '-' || c.is_whitespace()) && !prev_was_dash {
                prev_was_dash = true;
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.len() < 4 {
        generate_hostname().0
    } else {
        normalized.into()
    }
}

fn denormalize(s: &str) -> InternedString {
    let mut cap = true;
    s.chars()
        .map(|c| {
            if c == '-' {
                cap = true;
                ' '
            } else if cap {
                cap = false;
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect::<String>()
        .into()
}

impl ServerHostnameInfo {
    pub fn new(
        name: Option<InternedString>,
        hostname: Option<InternedString>,
    ) -> Result<Self, Error> {
        Self::new_opt(name, hostname)
            .map(|h| h.unwrap_or_else(|| ServerHostnameInfo::from_hostname(generate_hostname())))
    }

    pub fn new_opt(
        name: Option<InternedString>,
        hostname: Option<InternedString>,
    ) -> Result<Option<Self>, Error> {
        let name = name.filter(|n| !n.is_empty());
        let hostname = hostname.filter(|h| !h.is_empty());
        Ok(match (name, hostname) {
            (Some(name), Some(hostname)) => Some(ServerHostnameInfo {
                name,
                hostname: ServerHostname::new(hostname)?,
            }),
            (Some(name), None) => Some(ServerHostnameInfo::from_name(name)),
            (None, Some(hostname)) => Some(ServerHostnameInfo::from_hostname(ServerHostname::new(
                hostname,
            )?)),
            (None, None) => None,
        })
    }

    pub fn from_hostname(hostname: ServerHostname) -> Self {
        Self {
            name: denormalize(&**hostname),
            hostname,
        }
    }

    pub fn from_name(name: InternedString) -> Self {
        Self {
            hostname: ServerHostname(normalize(&*name)),
            name,
        }
    }

    pub fn load(server_info: &Model<ServerInfo>) -> Result<Self, Error> {
        Ok(Self {
            name: server_info.as_name().de()?,
            hostname: ServerHostname::load(server_info)?,
        })
    }

    pub fn save(&self, server_info: &mut Model<ServerInfo>) -> Result<(), Error> {
        server_info.as_name_mut().ser(&self.name)?;
        self.hostname.save(server_info)
    }
}

pub fn generate_hostname() -> ServerHostname {
    let num = rand::random::<u16>();
    ServerHostname(InternedString::from_display(&lazy_format!(
        "startos-{num:04x}"
    )))
}

pub fn generate_id() -> String {
    let id = uuid::Uuid::new_v4();
    id.to_string()
}

#[instrument(skip_all)]
pub async fn get_current_hostname() -> Result<InternedString, Error> {
    let out = Command::new("hostname")
        .invoke(ErrorKind::ParseSysInfo)
        .await?;
    let out_string = String::from_utf8(out)?;
    Ok(out_string.trim().into())
}

#[instrument(skip_all)]
pub async fn set_hostname(hostname: &ServerHostname) -> Result<(), Error> {
    hostname.validate()?;
    let hostname = &***hostname;
    // Set the hostname ourselves rather than via `hostnamectl`: it delegates the
    // static-file write to sandboxed systemd-hostnamed, which can't copy-up
    // /etc/hostname from the read-only squashfs lower (EACCES on the Pi kernel).
    // We already own /etc/hosts and persistence below, so the only thing we'd
    // lose is hostnamed's D-Bus change signal, whose one consumer (avahi) we
    // restart explicitly in sync_hostname.
    write_file_atomic("/etc/hostname", format!("{hostname}\n")).await?;
    nix::unistd::sethostname(hostname).map_err(|e| {
        Error::new(
            eyre!("failed to set live hostname: {e}"),
            ErrorKind::ParseSysInfo,
        )
    })?;
    Command::new("sed")
        .arg("-i")
        .arg(format!(
            "s/\\(\\s\\)localhost\\( {hostname}\\)\\?/\\1localhost {hostname}/g"
        ))
        .arg("/etc/hosts")
        .invoke(ErrorKind::ParseSysInfo)
        .await?;
    copy_file(
        "/etc/hostname",
        "/media/startos/config/overlay/etc/hostname",
    )
    .await?;
    copy_file("/etc/hosts", "/media/startos/config/overlay/etc/hosts").await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn sync_hostname(hostname: &ServerHostname) -> Result<(), Error> {
    set_hostname(hostname).await?;
    Command::new("systemctl")
        .arg("restart")
        .arg("avahi-daemon")
        .invoke(crate::ErrorKind::Network)
        .await?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[ts(export)]
pub struct SetServerHostnameParams {
    name: Option<InternedString>,
    hostname: Option<InternedString>,
}

pub async fn set_hostname_rpc(
    ctx: RpcContext,
    SetServerHostnameParams { name, hostname }: SetServerHostnameParams,
) -> Result<(), Error> {
    let name = name.filter(|n| !n.is_empty());
    let hostname = hostname
        .filter(|h| !h.is_empty())
        .map(ServerHostname::new)
        .transpose()?;
    if name.is_none() && hostname.is_none() {
        return Err(Error::new(
            eyre!("{}", t!("hostname.must-provide-name-or-hostname")),
            ErrorKind::InvalidRequest,
        ));
    };
    let info = ctx
        .db
        .mutate(|db| {
            let server_info = db.as_public_mut().as_server_info_mut();
            if let Some(name) = name {
                server_info.as_name_mut().ser(&name)?;
            }
            if let Some(hostname) = &hostname {
                hostname.save(server_info)?;
                server_info
                    .as_status_info_mut()
                    .as_restart_mut()
                    .ser(&Some(RestartReason::Mdns))?;
            }
            ServerHostnameInfo::load(server_info)
        })
        .await
        .result?;
    ctx.account.mutate(|a| a.hostname = info.clone());
    if let Some(h) = hostname {
        sync_hostname(&h).await?;
    }

    Ok(())
}

#[test]
fn test_generate_hostname() {
    assert_eq!(dbg!(generate_hostname().0).len(), 12);
}
