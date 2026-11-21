use std::collections::BTreeSet;
use std::path::Path;

use async_compression::tokio::bufread::GzipDecoder;
use serde::{Deserialize, Serialize};
use tokio::io::BufReader;
use tokio::process::Command;

use crate::PLATFORM;
use crate::disk::fsck::RequiresReboot;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::open_file;

pub(crate) const LIBREM_MINI_V2: &str = "librem_mini_v2";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum VersionPrefixes {
    One(String),
    Many(Vec<String>),
}

impl VersionPrefixes {
    fn strip_from<'a>(&self, version: &'a str) -> Option<&'a str> {
        let prefixes = match self {
            Self::One(prefix) => std::slice::from_ref(prefix),
            Self::Many(prefixes) => prefixes.as_slice(),
        };
        prefixes
            .iter()
            .find_map(|prefix| version.strip_prefix(prefix))
    }
}

/// SMBIOS version constraints for a bundled ROM.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VersionMatcher {
    semver_prefix: Option<VersionPrefixes>,
    semver_range: Option<semver::VersionReq>,
    semver_suffix: Option<String>,
}

/// Each bundled ROM has one manifest entry; BIOS prefixes are alternative selectors.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Firmware {
    id: String,
    platform: BTreeSet<String>,
    system_product_name: Option<String>,
    bios_version: Option<VersionMatcher>,
    /// SHA-256 of the compressed `.rom.gz` file.
    shasum: String,
}

pub fn display_firmware_update_result(result: RequiresReboot) {
    if result.0 {
        println!("Firmware successfully updated! Reboot to apply changes.");
    } else {
        println!("No firmware update available.");
    }
}

pub(crate) async fn system_product_name() -> Result<String, Error> {
    Ok(String::from_utf8(
        Command::new("dmidecode")
            .arg("-s")
            .arg("system-product-name")
            .invoke(ErrorKind::Firmware)
            .await?,
    )?
    .trim()
    .to_owned())
}

#[instrument]
pub async fn check_for_firmware_update() -> Result<Option<Firmware>, Error> {
    let system_product_name = system_product_name().await?;
    let bios_version = String::from_utf8(
        Command::new("dmidecode")
            .arg("-s")
            .arg("bios-version")
            .invoke(ErrorKind::Firmware)
            .await?,
    )?
    .trim()
    .to_owned();
    if system_product_name.is_empty() || bios_version.is_empty() {
        return Ok(None);
    }

    let firmwares = serde_json::from_str::<Vec<Firmware>>(
        &tokio::fs::read_to_string("/usr/lib/startos/firmware.json").await?,
    )
    .with_kind(ErrorKind::Deserialization)?;

    Ok(find_firmware_update(
        firmwares,
        &PLATFORM,
        &system_product_name,
        &bios_version,
    ))
}

fn find_firmware_update(
    firmwares: Vec<Firmware>,
    platform: &str,
    system_product_name: &str,
    bios_version: &str,
) -> Option<Firmware> {
    firmwares.into_iter().find(|firmware| {
        let matches_product_name = firmware
            .system_product_name
            .as_ref()
            .map_or(true, |spn| spn == system_product_name);
        let matches_bios_version = firmware
            .bios_version
            .as_ref()
            .map_or(Some(true), |bv| {
                let mut semver_str = bios_version;
                if let Some(prefixes) = &bv.semver_prefix {
                    semver_str = prefixes.strip_from(semver_str)?;
                }
                if let Some(suffix) = &bv.semver_suffix {
                    semver_str = semver_str.strip_suffix(suffix)?;
                }
                let semver = semver_str
                    .split(".")
                    .filter_map(|v| v.parse().ok())
                    .chain(std::iter::repeat(0))
                    .take(3)
                    .collect::<Vec<_>>();
                let semver = semver::Version::new(semver[0], semver[1], semver[2]);
                Some(
                    bv.semver_range
                        .as_ref()
                        .map_or(true, |r| r.matches(&semver)),
                )
            })
            .unwrap_or(false);
        firmware.platform.contains(platform) && matches_product_name && matches_bios_version
    })
}

/// Flashes the bundled ROM after verifying its compressed SHA-256.
#[instrument]
pub async fn update_firmware(firmware: Firmware) -> Result<(), Error> {
    let id = &firmware.id;
    let firmware_dir = Path::new("/usr/lib/startos/firmware");
    let filename = format!("{id}.rom.gz");
    let firmware_path = firmware_dir.join(&filename);
    Command::new("sha256sum")
        .arg("-c")
        .input(Some(&mut std::io::Cursor::new(format!(
            "{} {}",
            firmware.shasum,
            firmware_path.display()
        ))))
        .invoke(ErrorKind::Filesystem)
        .await?;
    let mut rdr = if tokio::fs::metadata(&firmware_path).await.is_ok() {
        GzipDecoder::new(BufReader::new(open_file(&firmware_path).await?))
    } else {
        return Err(Error::new(
            eyre!("Firmware {id}.rom.gz not found in {firmware_dir:?}"),
            ErrorKind::NotFound,
        ));
    };
    Command::new("flashrom")
        .arg("-p")
        .arg("internal")
        .arg("-w-")
        .input(Some(&mut rdr))
        .invoke(ErrorKind::Firmware)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_firmwares() -> Vec<Firmware> {
        serde_json::from_str(include_str!(
            "../../../../projects/start-os/build/lib/firmware.json"
        ))
        .unwrap()
    }

    fn shipped_update(bios_version: &str) -> Option<String> {
        find_firmware_update(shipped_firmwares(), "x86_64", LIBREM_MINI_V2, bios_version)
            .map(|firmware| firmware.id)
    }

    #[test]
    fn shipped_firmware_ids_are_unique() {
        let firmwares = shipped_firmwares();
        let ids: BTreeSet<_> = firmwares.iter().map(|firmware| &firmware.id).collect();
        assert_eq!(ids.len(), firmwares.len());
    }

    #[test]
    fn legacy_single_prefix_remains_supported() {
        let prefixes: VersionPrefixes = serde_json::from_str(r#""PureBoot-Release-""#).unwrap();
        assert_eq!(prefixes.strip_from("PureBoot-Release-29"), Some("29"));
        assert_eq!(prefixes.strip_from("PureBoot-start9-29"), None);
    }

    #[test]
    fn server_pure_updates_to_latest_pureboot() {
        let latest = Some("pureboot-librem_mini_v2-basic_usb_autoboot-start9-30.1.2".to_owned());
        assert_eq!(shipped_update("PureBoot-Release-28"), latest);
        assert_eq!(shipped_update("PureBoot-Release-29"), latest);
        assert_eq!(shipped_update("PureBoot-start9-30.1.1"), latest);
    }

    #[test]
    fn server_pure_on_latest_pureboot_is_left_alone() {
        assert_eq!(shipped_update("PureBoot-start9-30.1.2"), None);
        assert_eq!(shipped_update("PureBoot-Release-31"), None);
        assert_eq!(shipped_update("PureBoot-start9-31"), None);
    }
}
