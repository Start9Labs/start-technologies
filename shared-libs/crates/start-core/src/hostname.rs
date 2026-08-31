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

const MAX_LEN: usize = 32;

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

    /// Validates a user-supplied hostname.
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

    fn is_usable(&self) -> bool {
        self.validate().is_ok()
            && self.0.len() <= MAX_LEN
            && !self.0.starts_with('-')
            && !self.0.ends_with('-')
    }

    /// Treats an empty hostname as absent.
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

/// Returns a bootable, TLS-servable hostname, preserving usable stored names.
pub fn repair_hostname(stored: &str) -> ServerHostname {
    let stored = ServerHostname(InternedString::intern(stored));
    if stored.is_usable() {
        return stored;
    }
    let usable: String = stored
        .0
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let mut repaired: String = usable.trim_matches('-').chars().take(MAX_LEN).collect();
    while repaired.ends_with('-') {
        repaired.pop();
    }
    ServerHostname::new_from_input(InternedString::from_display(&repaired))
        .unwrap_or_else(|_| generate_hostname())
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
    // `systemd-hostnamed` cannot copy up `/etc/hostname` from the squashfs lower.
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
    /// The server's `.local` hostname: up to 32 lowercase letters, numbers, and
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
    fn input_enforces_the_operator_length_and_dns_label_edges() {
        validate_input(&"a".repeat(32)).unwrap();
        validate_input(&"a".repeat(33)).unwrap_err();
        validate_input("-my-server").unwrap_err();
        validate_input("my-server-").unwrap_err();
        validate_input("-").unwrap_err();
    }

    #[test]
    fn legacy_hostnames_load_before_repair() {
        validate(&"a".repeat(MAX_LEN + 1)).unwrap();
        validate("-my-server").unwrap();
    }

    #[test]
    fn the_longest_allowed_hostname_fits_the_root_ca_common_name() {
        let root_cert = |len: usize| {
            crate::net::ssl::make_root_cert(
                &crate::net::ssl::gen_nistp256().unwrap(),
                &crate::net::ssl::CertBranding::start_os(&"a".repeat(len)),
                std::time::SystemTime::now(),
            )
        };
        root_cert(MAX_LEN).unwrap();
    }

    #[test]
    fn the_longest_allowed_hostname_fits_a_leaf_common_name() {
        let root_key = crate::net::ssl::gen_nistp256().unwrap();
        let branding = crate::net::ssl::CertBranding::start_os("startos");
        let root =
            crate::net::ssl::make_root_cert(&root_key, &branding, std::time::SystemTime::now())
                .unwrap();
        let leaf_key = crate::net::ssl::gen_nistp256().unwrap();
        let leaf_cert = |len: usize| {
            let hostname = ServerHostname(InternedString::from_display(&"a".repeat(len)));
            let names = [hostname.local_domain_name()].into_iter().collect();
            crate::net::ssl::make_leaf_cert(
                (&root_key, &root),
                (&leaf_key, &crate::net::ssl::SANInfo::new(&names)),
                &branding,
            )
        };
        leaf_cert(MAX_LEN).unwrap();
    }

    #[test]
    fn generated_hostnames_are_valid_input() {
        validate_input(&generate_hostname()).unwrap();
    }

    #[test]
    fn repair_leaves_a_usable_hostname_alone() {
        assert_eq!(&*repair_hostname("my-cool-server"), "my-cool-server");
    }

    #[test]
    fn repair_keeps_as_much_of_an_unusable_hostname_as_it_can() {
        assert_eq!(&*repair_hostname("My_Cool Server"), "mycoolserver");
        assert_eq!(&*repair_hostname("-my-server-"), "my-server");
        assert_eq!(
            repair_hostname(&"a".repeat(MAX_LEN + 1)).chars().count(),
            MAX_LEN
        );
        assert_eq!(
            &*repair_hostname(&("-".repeat(MAX_LEN) + "my_server")),
            "myserver"
        );
    }

    #[test]
    fn repair_generates_a_hostname_when_nothing_usable_remains() {
        validate_input(&repair_hostname(&"_".repeat(70))).unwrap();
        validate_input(&repair_hostname("")).unwrap();
    }
}
