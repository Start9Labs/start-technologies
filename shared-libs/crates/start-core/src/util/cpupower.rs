use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use imbl::OrdMap;
use tokio::process::Command;
use ts_rs::TS;

use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::maybe_read_file_to_string;

pub const GOVERNOR_HEIRARCHY: &[Governor] = &[
    Governor(Cow::Borrowed("ondemand")),
    Governor(Cow::Borrowed("schedutil")),
    Governor(Cow::Borrowed("conservative")),
];

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, TS,
)]
#[ts(export, type = "string")]
pub struct Governor(Cow<'static, str>);
impl std::str::FromStr for Governor {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned().into()))
    }
}
impl std::fmt::Display for Governor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::ops::Deref for Governor {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl std::borrow::Borrow<str> for Governor {
    fn borrow(&self) -> &str {
        &**self
    }
}

pub async fn get_available_governors() -> Result<BTreeSet<Governor>, Error> {
    let raw = Command::new("cpupower")
        .arg("frequency-info")
        .arg("-g")
        .invoke(ErrorKind::CpuSettings)
        .await
        .map_or_else(|e| Ok(e.source.to_string()), String::from_utf8)?;
    let mut for_cpu: OrdMap<u32, BTreeSet<Governor>> = OrdMap::new();
    let mut current_cpu = None;
    for line in raw.lines() {
        if line.starts_with("analyzing") {
            current_cpu = Some(
                sscanf::sscanf!(line, "analyzing CPU {u32}:")
                    .map_err(|e| eyre!("{e}"))
                    .with_kind(ErrorKind::ParseSysInfo)?,
            );
        } else if let Some(rest) = line
            .trim()
            .strip_prefix("available cpufreq governors:")
            .map(|s| s.trim())
        {
            if rest != "Not Available" {
                for_cpu
                    .entry(current_cpu.ok_or_else(|| {
                        Error::new(
                            eyre!("{}", t!("util.cpupower.governors-listed-before-cpu")),
                            ErrorKind::ParseSysInfo,
                        )
                    })?)
                    .or_default()
                    .extend(
                        rest.split_ascii_whitespace()
                            .map(|g| Governor(Cow::Owned(g.to_owned()))),
                    );
            }
        }
    }
    Ok(for_cpu
        .into_iter()
        .fold(None, |acc: Option<BTreeSet<Governor>>, (_, x)| {
            if let Some(acc) = acc {
                Some(acc.intersection(&x).cloned().collect())
            } else {
                Some(x)
            }
        })
        .unwrap_or_default()) // include only governors available for ALL cpus
}

pub async fn current_governor() -> Result<Option<Governor>, Error> {
    let Some(raw) = Command::new("cpupower")
        .arg("frequency-info")
        .arg("-p")
        .env("LANG", "C.UTF-8")
        .invoke(ErrorKind::CpuSettings)
        .await
        .and_then(|s| Ok(Some(String::from_utf8(s)?)))
        .or_else(|e| {
            if e.source
                .to_string()
                .contains("Unable to determine current policy")
            {
                Ok(None)
            } else {
                Err(e)
            }
        })?
    else {
        return Ok(None);
    };

    for line in raw.lines() {
        if let Some(governor) = line
            .trim()
            .strip_prefix("The governor \"")
            .and_then(|s| s.strip_suffix("\" may decide which speed to use"))
        {
            return Ok(Some(Governor(Cow::Owned(governor.to_owned()))));
        }
    }
    Err(Error::new(
        eyre!(
            "{}",
            t!("util.cpupower.failed-to-parse-output", output = raw)
        ),
        ErrorKind::ParseSysInfo,
    ))
}

pub fn preferred_governor(available: &BTreeSet<Governor>) -> Option<&'static Governor> {
    GOVERNOR_HEIRARCHY
        .iter()
        .find(|governor| available.contains(*governor))
}

pub async fn set_governor(governor: &Governor) -> Result<(), Error> {
    Command::new("cpupower")
        .arg("frequency-set")
        .arg("-g")
        .arg(&*governor.0)
        .invoke(ErrorKind::CpuSettings)
        .await?;
    Ok(())
}

const CPU_ROOT: &str = "/sys/devices/system/cpu";

/// Selects how aggressively an EPP-capable CPU pursues performance.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, TS,
)]
#[ts(export, type = "string")]
pub struct Epp(Cow<'static, str>);

const LIBREM_MINI_V2: &str = "librem_mini_v2";
const LIBREM_MINI_V2_EPP: Epp = Epp(Cow::Borrowed("balance_power"));

impl std::str::FromStr for Epp {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned().into()))
    }
}
impl std::fmt::Display for Epp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::ops::Deref for Epp {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl std::borrow::Borrow<str> for Epp {
    fn borrow(&self) -> &str {
        &**self
    }
}

fn epp_path(cpu: &Path) -> PathBuf {
    cpu.join("cpufreq/energy_performance_preference")
}

async fn epp_paths() -> Result<Vec<PathBuf>, Error> {
    let mut dir = match tokio::fs::read_dir(CPU_ROOT).await {
        Ok(dir) => dir,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_kind(ErrorKind::Filesystem),
    };
    let mut paths = Vec::new();
    while let Some(entry) = dir.next_entry().await.with_kind(ErrorKind::Filesystem)? {
        let path = epp_path(&entry.path());
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

pub async fn get_available_epps() -> Result<BTreeSet<Epp>, Error> {
    let path = Path::new(CPU_ROOT).join("cpu0/cpufreq/energy_performance_available_preferences");
    Ok(maybe_read_file_to_string(path)
        .await?
        .into_iter()
        .flat_map(|raw| {
            raw.split_ascii_whitespace()
                .map(|e| Epp(Cow::Owned(e.to_owned())))
                .collect::<Vec<_>>()
        })
        .collect())
}

pub async fn current_epp() -> Result<Option<Epp>, Error> {
    Ok(
        maybe_read_file_to_string(epp_path(&Path::new(CPU_ROOT).join("cpu0")))
            .await?
            .map(|raw| Epp(Cow::Owned(raw.trim().to_owned()))),
    )
}

pub(crate) fn preferred_epp(
    selected: Option<Epp>,
    system_product_name: Option<&str>,
) -> Option<Epp> {
    selected.or_else(|| {
        (system_product_name == Some(LIBREM_MINI_V2)).then(|| LIBREM_MINI_V2_EPP.clone())
    })
}

pub async fn set_epp(epp: &Epp) -> Result<(), Error> {
    for path in epp_paths().await? {
        tokio::fs::write(&path, &*epp.0)
            .await
            .with_ctx(|_| (ErrorKind::CpuSettings, path.display().to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn librem_mini_v2_defaults_to_balance_power() {
        assert_eq!(
            preferred_epp(None, Some("librem_mini_v2")),
            Some(Epp(Cow::Borrowed("balance_power")))
        );
    }

    #[test]
    fn selected_epp_overrides_the_librem_mini_v2_default() {
        let selected = Epp(Cow::Borrowed("performance"));
        assert_eq!(
            preferred_epp(Some(selected.clone()), Some("librem_mini_v2")),
            Some(selected)
        );
    }

    #[test]
    fn other_products_have_no_default_epp() {
        assert_eq!(preferred_epp(None, Some("other")), None);
        assert_eq!(preferred_epp(None, None), None);
    }
}
