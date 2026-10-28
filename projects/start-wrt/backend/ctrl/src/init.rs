use uciedit::openwrt::{WifiDevice, WifiDynamicVlan, WifiInterface, WifiMode};
use uciedit::Arena;

use crate::prelude::*;
use crate::PASSWORD_CHARS;

const PASSWORD_LEN: usize = 12;

/// Validate that a password meets the sticker password requirements:
/// exactly 12 characters, every character in `PASSWORD_CHARS`.
fn validate_password(password: &str) -> Result<(), String> {
    if password.len() != PASSWORD_LEN {
        return Err(format!(
            "password must be exactly {PASSWORD_LEN} characters (got {})",
            password.len()
        ));
    }
    for ch in password.chars() {
        if !PASSWORD_CHARS.contains(ch) {
            return Err(format!("invalid character '{ch}' in password"));
        }
    }
    Ok(())
}

/// Hidden-input prompt with confirmation (operator-side manual password entry).
fn prompt_password_secure() -> Result<String, Error> {
    loop {
        let pw = rpassword::prompt_password("Enter WiFi password (12 chars from charset): ")
            .map_err(|e| {
                Error::new(eyre!("failed to read password: {e}"), ErrorKind::Filesystem)
            })?;

        if let Err(msg) = validate_password(&pw) {
            println!("Invalid: {msg}");
            continue;
        }

        let confirm = rpassword::prompt_password("Confirm: ").map_err(|e| {
            Error::new(eyre!("failed to read password: {e}"), ErrorKind::Filesystem)
        })?;

        if pw != confirm {
            println!("Passwords do not match. Try again.");
            continue;
        }

        return Ok(pw);
    }
}

/// Provision a WiFi password into `/etc/config/wireless`.
///
/// In default mode, generates a random 12-character password from
/// `PASSWORD_CHARS` and prints it to stdout (operator must record it).
/// In manual mode, prompts the operator (hidden input, with confirmation).
///
/// Either way, the password is written to UCI and `wifi reload` is invoked.
/// The password lives only in the overlay — factory reset wipes it, after
/// which `restore_wifi_if_needed` re-reads from EEPROM (or, if the EEPROM
/// has no tag 0x2F, leaves the AP unconfigured until this utility runs again).
pub async fn set_wifi_password(manual: bool) -> Result<(), Error> {
    let password = if manual {
        prompt_password_secure()?
    } else {
        let pw = crate::generate_password(PASSWORD_CHARS.as_bytes(), PASSWORD_LEN);
        println!("Generated WiFi password: {pw}");
        pw
    };

    println!("Configuring WiFi and reloading...");
    configure_wifi("/etc/config", &password, None).await?;

    if let Err(e) = crate::wifi::reload_wifi("/etc/config", crate::wifi::WifiReload::Reload).await {
        tracing::warn!("WiFi reload failed: {e}");
    }

    println!("WiFi AP is now active.");
    Ok(())
}

/// Configure WiFi: set SSID, encryption, password, and enable all radios.
///
/// Follows the UCI parse/modify/dump pattern from wifi.rs with conflict retry.
/// `uci_root` is typically "/etc/config" but can be a mounted eMMC path for
/// post-flash configuration.
///
/// `max_stations` optionally limits the number of associated stations per
/// interface (UCI `maxassoc`). Pass `Some(1)` in setup mode to restrict the
/// AP to a single client during reflash. Pass `None` for normal operation.
pub async fn configure_wifi(
    uci_root: &str,
    password: &str,
    max_stations: Option<u32>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        // Read raw bytes (Send future) — no arena yet
        let bytes = uciedit::read_all(uci_root, &["wireless"]).await?;
        // Parse + mutate + freeze in a sync scope that never crosses an await.
        // The arena is created, used, and dropped here — so the result
        // (FrozenConfigs) is fully Send and the enclosing future stays Send.
        let frozen = {
            let arena = Arena::new();
            let mut cfgs = bytes.parse(&arena)?;
            for s in &mut cfgs["wireless"].sections {
                if let Some(mut device) = s.get_typed::<WifiDevice>()? {
                    device.disabled = false;
                    s.set(&device)?;
                }
                if let Some(mut iface) = s.get_typed::<WifiInterface>()? {
                    if iface.mode != WifiMode::AP {
                        continue;
                    }
                    iface.ssid = "StartWRT".into();
                    iface.encryption = "psk2".into();
                    iface.key = Some(password.to_string());
                    iface.dynamic_vlan = WifiDynamicVlan::ALLOWED;
                    if let Some(max) = max_stations {
                        iface.maxassoc = Some(max);
                    }
                    s.set(&iface)?;
                }
            }
            cfgs.freeze()
        };
        match uciedit::write_all(uci_root, frozen).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(()) => return Ok(()),
        }
    }
}

/// Restore WiFi credentials from EEPROM tag 0x2F at boot time.
///
/// Runs only on a router with no admin password: a factory-fresh or reset
/// one. Returns `Ok(true)` if WiFi was configured, `Ok(false)` if an admin
/// password is set or no usable password is in EEPROM.
pub async fn restore_wifi_if_needed() -> Result<bool, Error> {
    if crate::captive::is_admin_password_set()? {
        return Ok(false);
    }

    let password = match crate::eeprom::read_wifi_password()? {
        Some(p) => p,
        None => {
            tracing::warn!(
                "no valid WiFi password in EEPROM tag 0x{:02X}; AP not configured. \
                 Provision a password over ethernet/serial.",
                crate::eeprom::TLV_TAG_WIFI_PMK,
            );
            return Ok(false);
        }
    };

    configure_wifi("/etc/config", &password, None).await?;

    if let Err(e) = crate::wifi::reload_wifi("/etc/config", crate::wifi::WifiReload::Reload).await {
        tracing::warn!("WiFi reload failed: {e}");
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use uciedit::parse_all;

    use super::*;

    #[tokio::test]
    async fn configure_wifi_keys_factory_config_under_another_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("wireless"),
            include_str!("../../firstboot_config/wireless"),
        )
        .unwrap();

        configure_wifi(dir.path().to_str().unwrap(), "Kf?J%3uZg8dm", None)
            .await
            .unwrap();

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["wireless"]).await.unwrap();
        let (mut radios, mut aps) = (0, 0);
        for s in &cfgs["wireless"].sections {
            if let Some(device) = s.get_typed::<WifiDevice>().unwrap() {
                radios += 1;
                assert!(!device.disabled);
            }
            if let Some(iface) = s.get_typed::<WifiInterface>().unwrap() {
                aps += 1;
                assert_eq!(iface.ssid, "StartWRT");
                assert_eq!(iface.encryption, "psk2");
                assert_eq!(iface.key.as_deref(), Some("Kf?J%3uZg8dm"));
                assert_eq!(iface.dynamic_vlan, WifiDynamicVlan::ALLOWED);
                assert_eq!(iface.maxassoc, None);
            }
        }
        assert_eq!((radios, aps), (2, 2));
    }
}
