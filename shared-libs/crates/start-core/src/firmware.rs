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

/// One installed BIOS version string a [`Firmware`] replaces
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VersionMatcher {
    /// Required at the start of the version string; stripped before comparing
    semver_prefix: Option<String>,
    /// Range the remaining dotted integers must fall in; unset accepts any
    semver_range: Option<semver::VersionReq>,
    /// Required at the end of the version string; stripped before comparing
    semver_suffix: Option<String>,
}
impl VersionMatcher {
    /// Segments that are not plain integers are dropped before the comparison.
    fn matches(&self, bios_version: &str) -> bool {
        let mut semver_str = bios_version;
        if let Some(prefix) = &self.semver_prefix {
            let Some(rest) = semver_str.strip_prefix(prefix) else {
                return false;
            };
            semver_str = rest;
        }
        if let Some(suffix) = &self.semver_suffix {
            let Some(rest) = semver_str.strip_suffix(suffix) else {
                return false;
            };
            semver_str = rest;
        }
        let semver = semver_str
            .split(".")
            .filter_map(|v| v.parse().ok())
            .chain(std::iter::repeat(0))
            .take(3)
            .collect::<Vec<_>>();
        let semver = semver::Version::new(semver[0], semver[1], semver[2]);
        self.semver_range
            .as_ref()
            .map_or(true, |r| r.matches(&semver))
    }
}

/// One entry of `/usr/lib/startos/firmware.json`: an image and the machines it is flashed onto
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Firmware {
    id: String,
    /// StartOS platforms the image applies to
    platform: BTreeSet<String>,
    /// `dmidecode -s system-product-name` of the machine; unset matches any
    system_product_name: Option<String>,
    /// Installed `dmidecode -s bios-version` strings the image replaces; empty replaces any
    #[serde(default)]
    bios_version: Vec<VersionMatcher>,
    /// SHA-256 of the `.rom.gz`
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

    for firmware in serde_json::from_str::<Vec<Firmware>>(
        &tokio::fs::read_to_string("/usr/lib/startos/firmware.json").await?,
    )
    .with_kind(ErrorKind::Deserialization)?
    {
        let matches_product_name = firmware
            .system_product_name
            .as_ref()
            .map_or(true, |spn| spn == &system_product_name);
        let matches_bios_version = firmware.bios_version.is_empty()
            || firmware
                .bios_version
                .iter()
                .any(|bv| bv.matches(&bios_version));
        if firmware.platform.contains(&*PLATFORM) && matches_product_name && matches_bios_version {
            return Ok(Some(firmware));
        }
    }

    Ok(None)
}

/// Flashes the image from `/usr/lib/startos/firmware`; takes effect on the next boot.
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

    fn matcher(prefix: &str, range: Option<&str>) -> VersionMatcher {
        VersionMatcher {
            semver_prefix: Some(prefix.to_owned()),
            semver_range: range.map(|r| r.parse().unwrap()),
            semver_suffix: None,
        }
    }

    #[test]
    fn prefix_alone_matches_every_version_carrying_it() {
        let purism = matcher("PureBoot-Release-", None);
        assert!(purism.matches("PureBoot-Release-29"));
        assert!(purism.matches("PureBoot-Release-30.1"));
        assert!(purism.matches("PureBoot-Release-24-USBAutoboot-3"));
        assert!(!purism.matches("PureBoot-start9-30.1.1"));
        assert!(!purism.matches("4.22.01-Purism-1"));
    }

    #[test]
    fn range_compares_the_dotted_integers_after_the_prefix() {
        let start9 = matcher("PureBoot-start9-", Some("<30.1.1"));
        assert!(start9.matches("PureBoot-start9-30.1.0"));
        assert!(start9.matches("PureBoot-start9-30.1"));
        assert!(!start9.matches("PureBoot-start9-30.1.1"));
        assert!(!start9.matches("PureBoot-start9-31.0.1"));
        assert!(!start9.matches("PureBoot-Release-29"));
    }

    #[test]
    fn segments_that_are_not_integers_are_dropped() {
        let start9 = matcher("PureBoot-start9-", Some("<30.1.1"));
        assert!(start9.matches("PureBoot-start9-30.1.1-rc1"));
    }

    #[test]
    fn shipped_firmware_json_deserializes() {
        let firmwares: Vec<Firmware> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../projects/start-os/build/lib/firmware.json"
        )))
        .unwrap();
        for firmware in &firmwares {
            assert!(firmware.platform.contains("x86_64"), "{}", firmware.id);
            assert!(!firmware.bios_version.is_empty(), "{}", firmware.id);
        }
    }
}
