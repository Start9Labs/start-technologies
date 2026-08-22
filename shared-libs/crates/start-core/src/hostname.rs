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

/// The root CA's Common Name is `<hostname> Local Root CA`, and X.509 caps a Common
/// Name at 64 characters.
const MAX_LEN: usize = 50;

impl ServerHostname {
    /// Checks the charset alone, so a hostname stored before the other rules
    /// existed still loads.
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

    /// Checks a hostname the operator supplied against every rule, including the
    /// ones `new` leaves to this method so that an already-stored hostname loads.
    pub fn new_from_input(hostname: InternedString) -> Result<Self, Error> {
        let res = Self::new(hostname)?;
        if res.0.chars().count() > MAX_LEN {
            return Err(Error::new(
                eyre!("{}", t!("hostname.too-long", max = MAX_LEN)),
                ErrorKind::InvalidRequest,
            ));
        }
        if res.0.starts_with('-') || res.0.ends_with('-') {
            return Err(Error::new(
                eyre!("{}", t!("hostname.hyphen-edge")),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok(res)
    }

    /// Treats an empty hostname as absent rather than invalid.
    pub fn new_opt(hostname: Option<InternedString>) -> Result<Option<Self>, Error> {
        hostname
            .filter(|h| !h.is_empty())
            .map(Self::new_from_input)
            .transpose()
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
    /// The server's `.local` hostname: up to 50 lowercase letters, numbers, and
    /// hyphens, not starting or ending with a hyphen
    #[arg(help = "help.arg.hostname")]
    hostname: InternedString,
}

pub async fn set_hostname_rpc(
    ctx: RpcContext,
    SetServerHostnameParams { hostname }: SetServerHostnameParams,
) -> Result<(), Error> {
    let hostname = ServerHostname::new_from_input(hostname)?;
    ctx.db
        .mutate(|db| {
            let server_info = db.as_public_mut().as_server_info_mut();
            hostname.save(server_info)?;
            server_info
                .as_status_info_mut()
                .as_restart_mut()
                .ser(&Some(RestartReason::Mdns))
        })
        .await
        .result?;
    ctx.account.mutate(|a| a.hostname = hostname.clone());
    sync_hostname(&hostname).await?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn validate(hostname: &str) -> Result<(), Error> {
        ServerHostname::new(InternedString::intern(hostname)).map(|_| ())
    }

    fn validate_input(hostname: &str) -> Result<(), Error> {
        ServerHostname::new_from_input(InternedString::intern(hostname)).map(|_| ())
    }

    #[test]
    fn test_generate_hostname() {
        let generated = dbg!(generate_hostname());
        assert_eq!(generated.0.len(), 12);
        generated.validate().unwrap();
    }

    #[test]
    fn accepts_lowercase_digits_and_hyphens() {
        validate("my-cool-server-2").unwrap();
    }

    #[test]
    fn rejects_empty_uppercase_spaces_and_underscores() {
        validate("").unwrap_err();
        validate("My Cool Server").unwrap_err();
        validate("my_cool_server").unwrap_err();
    }

    #[test]
    fn input_rejects_a_label_no_dns_would_carry() {
        validate_input(&"a".repeat(MAX_LEN)).unwrap();
        validate_input(&"a".repeat(MAX_LEN + 1)).unwrap_err();
        validate_input("-my-server").unwrap_err();
        validate_input("my-server-").unwrap_err();
        validate_input("-").unwrap_err();
    }

    // `new` checks the charset alone, so a hostname stored before the other rules
    // existed still loads.
    #[test]
    fn stored_hostnames_are_held_to_the_charset_alone() {
        validate(&"a".repeat(MAX_LEN + 1)).unwrap();
        validate("-my-server").unwrap();
    }

    // `MAX_LEN` exists to keep the root CA's Common Name inside X.509's limit, so
    // it has to move whenever the branding around the hostname does.
    #[test]
    fn the_longest_hostname_still_fits_the_root_ca_common_name() {
        let cn = crate::net::ssl::CertBranding::start_os(&"a".repeat(MAX_LEN)).root_ca_cn;
        assert!(cn.len() <= 64, "root CA CN is {} characters", cn.len());
    }

    #[test]
    fn generated_hostnames_are_valid_input() {
        validate_input(&generate_hostname()).unwrap();
    }
}
