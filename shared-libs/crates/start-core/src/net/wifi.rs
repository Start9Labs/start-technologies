use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::time::Duration;

use clap::Parser;
use clap::builder::TypedValueParser;
use isocountry::CountryCode;
use lazy_static::lazy_static;
use regex::Regex;
use rpc_toolkit::{Context, Empty, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::{RwLockMappedWriteGuard, RwLockReadGuard, RwLockWriteGuard};
use tracing::instrument;
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::{NetworkInterfaceType, WifiInfo};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::{Error, ErrorKind, GatewayId};

impl RpcContext {
    async fn read_wifi_manager(&self) -> Result<RwLockReadGuard<'_, WpaCli>, Error> {
        let err = || {
            Error::new(
                color_eyre::eyre::eyre!("{}", t!("net.wifi.no-interface-available")),
                ErrorKind::Wifi,
            )
        };
        let Some(interface) = self
            .db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_network()
            .as_wifi()
            .as_interface()
            .de()?
        else {
            return Err(err());
        };
        let mut cli = RwLockReadGuard::try_map(self.wifi_manager.read().await, |c| c.as_ref()).ok();
        while cli.as_ref().map_or(true, |c| c.interface != interface) {
            drop(cli.take());
            let mut guard = self.wifi_manager.write().await;
            *guard = Some(WpaCli::new(interface.clone()));
            drop(guard);
            cli = RwLockReadGuard::try_map(self.wifi_manager.read().await, |c| c.as_ref()).ok();
        }
        cli.ok_or_else(err)
    }

    async fn write_wifi_manager(&self) -> Result<RwLockMappedWriteGuard<'_, WpaCli>, Error> {
        let err = || {
            Error::new(
                color_eyre::eyre::eyre!("{}", t!("net.wifi.no-interface-available")),
                ErrorKind::Wifi,
            )
        };
        let Some(interface) = self
            .db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_network()
            .as_wifi()
            .as_interface()
            .de()?
        else {
            return Err(err());
        };
        let mut cli = self.wifi_manager.write().await;
        if cli.as_ref().map_or(true, |c| c.interface != interface) {
            *cli = Some(WpaCli::new(interface));
        }
        RwLockWriteGuard::try_map(cli, |c| c.as_mut()).map_err(|_| err())
    }

    async fn ethernet_interface_connected(&self) -> Result<bool, Error> {
        for (iface, info) in self
            .db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_network()
            .as_gateways()
            .as_entries()?
        {
            let Some(info) = info.as_ip_info().transpose_ref() else {
                continue;
            };
            if info.as_deref().as_device_type().de()? == Some(NetworkInterfaceType::Ethernet) {
                if interface_connected(iface.as_str()).await? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

pub fn wifi<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "set-enabled",
            from_fn_async(set_enabled)
                .no_display()
                .with_about("about.enable-disable-wifi")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add)
                .no_display()
                .with_about("about.add-wifi-ssid-password")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "connect",
            from_fn_async(connect)
                .no_display()
                .with_about("about.connect-wifi-network")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .no_display()
                .with_about("about.remove-wifi-network")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "get",
            from_fn_async(get)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_wifi_info(handle.params, result))
                .with_about("about.list-wifi-info")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "country",
            country::<C>().with_about("about.command-set-country"),
        )
        .subcommand(
            "available",
            available::<C>().with_about("about.command-list-available-wifi"),
        )
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SetWifiEnabledParams {
    #[arg(long, help = "help.arg.wifi-enabled")]
    pub enabled: bool,
}

pub async fn set_enabled(
    ctx: RpcContext,
    SetWifiEnabledParams { enabled }: SetWifiEnabledParams,
) -> Result<(), Error> {
    if enabled {
        Command::new("rfkill")
            .arg("unblock")
            .arg("all")
            .invoke(ErrorKind::Wifi)
            .await?;
    } else {
        Command::new("rfkill")
            .arg("block")
            .arg("all")
            .invoke(ErrorKind::Wifi)
            .await?;
    }
    ctx.db
        .mutate(|d| {
            d.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_wifi_mut()
                .as_enabled_mut()
                .ser(&enabled)
        })
        .await
        .result?;
    Ok(())
}

pub fn available<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "get",
        from_fn_async(get_available)
            .with_display_serializable()
            .with_custom_display_fn(|handle, result| display_wifi_list(handle.params, result))
            .with_about("about.list-available-wifi-networks")
            .with_call_remote::<CliContext>(),
    )
}

pub fn country<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "set",
        from_fn_async(set_country)
            .no_display()
            .with_about("about.set-country")
            .with_call_remote::<CliContext>(),
    )
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct WifiAddParams {
    #[arg(help = "help.arg.wifi-ssid")]
    ssid: String,
    #[arg(help = "help.arg.wifi-password")]
    password: String,
}
#[instrument(skip_all)]
pub async fn add(
    ctx: RpcContext,
    WifiAddParams { ssid, password }: WifiAddParams,
) -> Result<(), Error> {
    let mut wpa_supplicant = ctx.write_wifi_manager().await?;
    let ssid = Ssid(ssid);
    if !password.is_ascii() {
        return Err(Error::new(
            color_eyre::eyre::eyre!("{}", t!("net.wifi.password-no-special-characters")),
            ErrorKind::Wifi,
        ));
    }
    if let Err(err) = wpa_supplicant
        .add_network(ctx.db.clone(), &ssid, &Psk(password))
        .await
    {
        tracing::error!(
            "{}",
            t!(
                "net.wifi.failed-to-add-network",
                ssid = &ssid.0,
                error = err.to_string()
            )
        );
        tracing::debug!("{:?}", err);
        return Err(Error::new(
            color_eyre::eyre::eyre!("{}", t!("net.wifi.failed-adding", ssid = &ssid.0)),
            ErrorKind::Wifi,
        ));
    }
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_wifi_mut()
                .as_ssids_mut()
                .mutate(|s| {
                    s.insert(ssid.0);
                    Ok(())
                })
        })
        .await
        .result?;
    Ok(())
}
#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct WifiSsidParams {
    #[arg(help = "help.arg.wifi-ssid")]
    ssid: String,
}

#[instrument(skip_all)]
pub async fn connect(
    ctx: RpcContext,
    WifiSsidParams { ssid }: WifiSsidParams,
) -> Result<(), Error> {
    let mut wpa_supplicant = ctx.write_wifi_manager().await?;
    if let Err(err) = async {
        let current = wpa_supplicant.get_current_network().await?;
        let connected = wpa_supplicant
            .select_network(ctx.db.clone(), &Ssid(ssid.clone()))
            .await?;
        if connected {
            tracing::info!("{}", t!("net.wifi.connected-successfully", ssid = &ssid));
        } else {
            tracing::info!("{}", t!("net.wifi.connection-failed", ssid = &ssid));
            match current {
                None => {
                    tracing::info!("{}", t!("net.wifi.no-wifi-to-revert"));
                }
                Some(current) => {
                    wpa_supplicant
                        .select_network(ctx.db.clone(), &current)
                        .await?;
                }
            }
        }
        Ok::<_, Error>(())
    }
    .await
    {
        tracing::error!(
            "{}",
            t!(
                "net.wifi.failed-to-connect",
                ssid = &ssid,
                error = err.to_string()
            )
        );
        return Err(Error::new(
            color_eyre::eyre::eyre!("{}", t!("net.wifi.cant-connect", ssid = &ssid)),
            ErrorKind::Wifi,
        ));
    }

    ctx.db
        .mutate(|db| {
            let wifi = db
                .as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_wifi_mut();
            wifi.as_ssids_mut().mutate(|s| {
                s.insert(ssid.clone());
                Ok(())
            })?;
            wifi.as_selected_mut().ser(&Some(ssid))
        })
        .await
        .result?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn remove(ctx: RpcContext, WifiSsidParams { ssid }: WifiSsidParams) -> Result<(), Error> {
    let mut wpa_supplicant = ctx.write_wifi_manager().await?;
    let current = wpa_supplicant.get_current_network().await?;
    let ssid = Ssid(ssid);
    let is_current_being_removed = matches!(current, Some(current) if current == ssid);
    let is_current_removed_and_no_hardwire =
        is_current_being_removed && !ctx.ethernet_interface_connected().await?;
    if is_current_removed_and_no_hardwire {
        return Err(Error::new(
            color_eyre::eyre::eyre!("{}", t!("net.wifi.forbidden-delete-would-disconnect")),
            ErrorKind::Wifi,
        ));
    }

    wpa_supplicant.remove_network(ctx.db.clone(), &ssid).await?;

    ctx.db
        .mutate(|db| {
            let wifi = db
                .as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_wifi_mut();
            wifi.as_ssids_mut().mutate(|s| {
                s.remove(&ssid.0);
                Ok(())
            })?;
            wifi.as_selected_mut()
                .map_mutate(|s| Ok(s.filter(|s| s == &ssid.0)))
        })
        .await
        .result?;
    Ok(())
}
#[derive(serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WifiListInfo {
    ssids: HashMap<Ssid, SignalStrength>,
    connected: Option<Ssid>,
    #[ts(type = "string | null")]
    country: Option<CountryCode>,
    ethernet: bool,
    available_wifi: Vec<WifiListOut>,
}
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WifiListInfoLow {
    strength: SignalStrength,
    security: Vec<String>,
}
#[derive(serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WifiListOut {
    ssid: Ssid,
    strength: SignalStrength,
    security: Vec<String>,
}
pub type WifiList = HashMap<Ssid, WifiListInfoLow>;
fn display_wifi_info(params: WithIoFormat<Empty>, info: WifiListInfo) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, info);
    }

    let mut table_global = Table::new();
    table_global.add_row(row![bc =>
        "CONNECTED",
        "SIGNAL STRENGTH",
        "COUNTRY",
        "ETHERNET",
    ]);
    table_global.add_row(row![
        &info
            .connected
            .as_ref()
            .map_or("N/A".to_owned(), |c| c.0.clone()),
        &info
            .connected
            .as_ref()
            .and_then(|x| info.ssids.get(x))
            .map_or("N/A".to_owned(), |ss| format!("{}", ss.0)),
        info.country.as_ref().map(|c| c.alpha2()).unwrap_or("00"),
        &format!("{}", info.ethernet)
    ]);
    table_global.print_tty(false).unwrap();

    let mut table_ssids = Table::new();
    table_ssids.add_row(row![bc => "SSID", "STRENGTH"]);
    for (ssid, signal_strength) in &info.ssids {
        let mut row = row![&ssid.0, format!("{}", signal_strength.0)];
        row.iter_mut()
            .map(|c| {
                c.style(Attr::ForegroundColor(match &signal_strength.0 {
                    x if x >= &90 => color::GREEN,
                    x if x == &50 => color::MAGENTA,
                    x if x == &0 => color::RED,
                    _ => color::YELLOW,
                }))
            })
            .for_each(drop);
        table_ssids.add_row(row);
    }
    table_ssids.print_tty(false).unwrap();

    let mut table_global = Table::new();
    table_global.add_row(row![bc =>
        "SSID",
        "STRENGTH",
        "SECURITY",
    ]);
    for table_info in info.available_wifi {
        table_global.add_row(row![
            &table_info.ssid.0,
            &format!("{}", table_info.strength.0),
            &table_info.security.join(" ")
        ]);
    }

    table_global.print_tty(false)?;
    Ok(())
}

fn display_wifi_list(params: WithIoFormat<Empty>, info: Vec<WifiListOut>) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, info);
    }

    let mut table_global = Table::new();
    table_global.add_row(row![bc =>
        "SSID",
        "STRENGTH",
        "SECURITY",
    ]);
    for table_info in info {
        table_global.add_row(row![
            &table_info.ssid.0,
            &format!("{}", table_info.strength.0),
            &table_info.security.join(" ")
        ]);
    }

    table_global.print_tty(false)?;
    Ok(())
}

// #[command(display(display_wifi_info))]
#[instrument(skip_all)]
pub async fn get(ctx: RpcContext, _: Empty) -> Result<WifiListInfo, Error> {
    let wpa_supplicant = ctx.read_wifi_manager().await?;
    let (list_networks, current_res, country_res, ethernet_res, signal_strengths) = tokio::join!(
        wpa_supplicant.list_networks_low(),
        wpa_supplicant.get_current_network(),
        wpa_supplicant.get_country_low(),
        ctx.ethernet_interface_connected(),
        wpa_supplicant.list_wifi_low()
    );
    let signal_strengths = signal_strengths?;
    let list_networks: BTreeSet<_> = list_networks?.into_iter().map(|(_, x)| x.ssid).collect();
    let available_wifi = {
        let mut wifi_list: Vec<WifiListOut> = signal_strengths
            .clone()
            .into_iter()
            .filter(|(ssid, _)| !list_networks.contains(ssid))
            .map(|(ssid, info)| WifiListOut {
                ssid,
                strength: info.strength,
                security: info.security,
            })
            .collect();
        wifi_list.sort_by_key(|x| x.strength);
        wifi_list.reverse();
        wifi_list
    };
    let ssids: HashMap<Ssid, SignalStrength> = list_networks
        .into_iter()
        .map(|ssid| {
            let signal_strength = signal_strengths
                .get(&ssid)
                .map(|x| x.strength)
                .unwrap_or_default();
            (ssid, signal_strength)
        })
        .collect();
    let current = current_res?;
    Ok(WifiListInfo {
        ssids,
        connected: current,
        country: country_res?,
        ethernet: ethernet_res?,
        available_wifi,
    })
}

#[instrument(skip_all)]
pub async fn get_available(ctx: RpcContext, _: Empty) -> Result<Vec<WifiListOut>, Error> {
    let wpa_supplicant = ctx.read_wifi_manager().await?;
    let (wifi_list, network_list) = tokio::join!(
        wpa_supplicant.list_wifi_low(),
        wpa_supplicant.list_networks_low()
    );
    let network_list = network_list?
        .into_iter()
        .map(|(_, info)| info.ssid)
        .collect::<BTreeSet<_>>();
    let mut wifi_list: Vec<WifiListOut> = wifi_list?
        .into_iter()
        .filter(|(ssid, _)| !network_list.contains(ssid))
        .map(|(ssid, info)| WifiListOut {
            ssid,
            strength: info.strength,
            security: info.security,
        })
        .collect();
    wifi_list.sort_by_key(|x| x.strength);
    wifi_list.reverse();
    Ok(wifi_list)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SetCountryParams {
    #[arg(value_parser = CountryCodeParser, help = "help.arg.wifi-country-code")]
    #[ts(type = "string")]
    country: CountryCode,
}
pub async fn set_country(
    ctx: RpcContext,
    SetCountryParams { country }: SetCountryParams,
) -> Result<(), Error> {
    if !ctx.ethernet_interface_connected().await? {
        return Err(Error::new(
            color_eyre::eyre::eyre!("{}", t!("net.wifi.wont-change-country-without-ethernet")),
            crate::ErrorKind::Wifi,
        ));
    }
    let mut wpa_supplicant = ctx.write_wifi_manager().await?;
    wpa_supplicant.set_country_low(country.alpha2()).await?;
    for (network_id, _wifi_info) in wpa_supplicant.list_networks_low().await? {
        wpa_supplicant.remove_network_low(network_id).await?;
    }
    wpa_supplicant.remove_all_connections().await?;

    wpa_supplicant.save_config(ctx.db.clone()).await?;

    Ok(())
}

#[derive(Debug)]
pub struct WpaCli {
    interface: GatewayId,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkId(String);

/// Ssid are the names of the wifis, usually human readable.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize, TS,
)]
pub struct Ssid(String);

/// So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    TS,
)]
pub struct SignalStrength(u8);

impl SignalStrength {
    fn new(size: Option<u8>) -> Self {
        let size = match size {
            None => return Self(0),
            Some(x) => x,
        };
        if size >= 100 {
            return Self(100);
        }
        Self(size)
    }
}

#[derive(Debug, Clone)]
pub struct WifiInfoLow {
    ssid: Ssid,
    device: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Psk(String);

/// Split a line of `nmcli -t`/`-g` terse output into fields, honoring nmcli's
/// backslash escaping (`\:` is a literal colon, `\\` a literal backslash) so an
/// SSID containing `:` stays one field instead of being split apart.
fn split_nmcli_terse_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => field.push(chars.next().unwrap_or('\\')),
            ':' => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    fields
}

impl WpaCli {
    pub fn new(interface: GatewayId) -> Self {
        WpaCli { interface }
    }

    #[instrument(skip_all)]
    pub async fn set_add_network_low(&mut self, ssid: &Ssid, psk: &Psk) -> Result<(), Error> {
        let _ = Command::new("nmcli")
            .arg("-a")
            .arg("-w")
            .arg("30")
            .arg("d")
            .arg("wifi")
            .arg("con")
            .arg(&ssid.0)
            .arg("password")
            .arg(&psk.0)
            .invoke(ErrorKind::Wifi)
            .await?;
        Ok(())
    }
    #[instrument(skip_all)]
    pub async fn add_network_low(&mut self, ssid: &Ssid, psk: &Psk) -> Result<(), Error> {
        if self.find_networks(ssid).await?.is_empty() {
            Command::new("nmcli")
                .arg("con")
                .arg("add")
                .arg("con-name")
                .arg(&ssid.0)
                .arg("type")
                .arg("wifi")
                .arg("ssid")
                .arg(&ssid.0)
                .invoke(ErrorKind::Wifi)
                .await?;
        }
        Command::new("nmcli")
            .arg("con")
            .arg("modify")
            .arg(&ssid.0)
            .arg("wifi-sec.key-mgmt")
            .arg("wpa-psk")
            .invoke(ErrorKind::Wifi)
            .await?;
        Command::new("nmcli")
            .arg("con")
            .arg("modify")
            .arg(&ssid.0)
            .arg("ifname")
            .arg(self.interface.as_str())
            .invoke(ErrorKind::Wifi)
            .await
            .map(|_| ())
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "{}",
                    t!(
                        "net.wifi.failed-to-set-interface",
                        interface = &self.interface,
                        ssid = &ssid.0
                    )
                );
                tracing::debug!("{:?}", e);
            });
        Command::new("nmcli")
            .arg("con")
            .arg("modify")
            .arg(&ssid.0)
            .arg("wifi-sec.psk")
            .arg(&psk.0)
            .invoke(ErrorKind::Wifi)
            .await?;
        Ok(())
    }
    pub async fn set_country_low(&mut self, country_code: &str) -> Result<(), Error> {
        let _ = Command::new("iw")
            .arg("reg")
            .arg("set")
            .arg(country_code)
            .invoke(ErrorKind::Wifi)
            .await?;

        Ok(())
    }
    pub async fn get_country_low(&self) -> Result<Option<CountryCode>, Error> {
        let r = Command::new("iw")
            .arg("reg")
            .arg("get")
            .invoke(ErrorKind::Wifi)
            .await?;
        let r = String::from_utf8(r)?;
        lazy_static! {
            static ref RE: Regex = Regex::new("country (\\w+):").unwrap();
        }
        let first_country = r.lines().find(|s| s.contains("country")).ok_or_else(|| {
            Error::new(
                color_eyre::eyre::eyre!("{}", t!("net.wifi.could-not-find-country-config")),
                ErrorKind::Wifi,
            )
        })?;
        let country = &RE.captures(first_country).ok_or_else(|| {
            Error::new(
                color_eyre::eyre::eyre!("{}", t!("net.wifi.could-not-parse-country-config")),
                ErrorKind::Wifi,
            )
        })?[1];
        if country == "00" {
            Ok(None)
        } else {
            Ok(Some(CountryCode::for_alpha2(country).map_err(|_| {
                Error::new(
                    color_eyre::eyre::eyre!(
                        "{}",
                        t!("net.wifi.invalid-country-code", country = country)
                    ),
                    ErrorKind::Wifi,
                )
            })?))
        }
    }
    pub async fn remove_network_low(&mut self, id: NetworkId) -> Result<(), Error> {
        let _ = Command::new("nmcli")
            .arg("c")
            .arg("del")
            .arg(&id.0)
            .invoke(ErrorKind::Wifi)
            .await?;
        Ok(())
    }
    #[instrument(skip_all)]
    pub async fn list_networks_low(&self) -> Result<BTreeMap<NetworkId, WifiInfoLow>, Error> {
        let r = Command::new("nmcli")
            .arg("-t")
            .arg("c")
            .arg("show")
            .invoke(ErrorKind::Wifi)
            .await?;
        let r = String::from_utf8(r)?;
        Ok(r.lines()
            .filter_map(|l| {
                let mut fields = split_nmcli_terse_line(l).into_iter();
                let name = Ssid(fields.next()?);
                let uuid = NetworkId(fields.next()?);
                let connection_type = fields.next()?;
                let device = fields.next();
                if !connection_type.contains("wireless") {
                    return None;
                }
                let info = WifiInfoLow { ssid: name, device };
                Some((uuid, info))
            })
            .collect::<BTreeMap<NetworkId, WifiInfoLow>>())
    }

    #[instrument(skip_all)]
    pub async fn list_wifi_low(&self) -> Result<WifiList, Error> {
        let r = Command::new("nmcli")
            .arg("-g")
            .arg("SSID,SIGNAL,security")
            .arg("d")
            .arg("wifi")
            .arg("list")
            .invoke(ErrorKind::Wifi)
            .await?;
        Ok(String::from_utf8(r)?
            .lines()
            .filter_map(|l| {
                let mut values = split_nmcli_terse_line(l).into_iter();
                let ssid = Ssid(values.next()?);
                let signal =
                    SignalStrength::new(std::str::FromStr::from_str(values.next()?.as_str()).ok());
                let security: Vec<String> =
                    values.next()?.split(' ').map(|x| x.to_owned()).collect();
                Some((
                    ssid,
                    WifiListInfoLow {
                        strength: signal,
                        security,
                    },
                ))
            })
            .collect::<WifiList>())
    }
    pub async fn select_network_low(&mut self, id: &NetworkId) -> Result<(), Error> {
        let _ = Command::new("nmcli")
            .arg("c")
            .arg("up")
            .arg(&id.0)
            .invoke(ErrorKind::Wifi)
            .await?;
        Ok(())
    }
    pub async fn remove_all_connections(&mut self) -> Result<(), Error> {
        let location_connections = Path::new("/etc/NetworkManager/system-connections");
        let mut connections = tokio::fs::read_dir(&location_connections).await?;
        while let Some(connection) = connections.next_entry().await? {
            let path = connection.path();
            if path.is_file() {
                let _ = tokio::fs::remove_file(&path).await?;
            }
        }

        Ok(())
    }
    pub async fn save_config(&mut self, db: TypedPatchDb<Database>) -> Result<(), Error> {
        let new_country = self.get_country_low().await?;
        db.mutate(|d| {
            d.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_wifi_mut()
                .as_last_region_mut()
                .ser(&new_country)
        })
        .await
        .result
    }
    async fn check_active_network(&self, ssid: &Ssid) -> Result<Option<NetworkId>, Error> {
        Ok(self
            .list_networks_low()
            .await?
            .iter()
            .find_map(|(network_id, wifi_info)| {
                wifi_info.device.as_ref()?;
                if wifi_info.ssid == *ssid {
                    Some(network_id.clone())
                } else {
                    None
                }
            }))
    }
    pub async fn find_networks(&self, ssid: &Ssid) -> Result<Vec<NetworkId>, Error> {
        Ok(self
            .list_networks_low()
            .await?
            .iter()
            .filter_map(|(network_id, wifi_info)| {
                if wifi_info.ssid == *ssid {
                    Some(network_id.clone())
                } else {
                    None
                }
            })
            .collect())
    }
    #[instrument(skip_all)]
    pub async fn select_network(
        &mut self,
        db: TypedPatchDb<Database>,
        ssid: &Ssid,
    ) -> Result<bool, Error> {
        let m_id = self.check_active_network(ssid).await?;
        match m_id {
            None => Err(Error::new(
                color_eyre::eyre::eyre!("{}", t!("net.wifi.ssid-not-found")),
                ErrorKind::Wifi,
            )),
            Some(x) => {
                self.select_network_low(&x).await?;
                self.save_config(db).await?;
                let connect = async {
                    let mut current;
                    loop {
                        current = self.get_current_network().await;
                        if let Ok(Some(ssid)) = &current {
                            tracing::debug!("Connected to: {}", ssid.0);
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        tracing::debug!("Retrying...");
                    }
                    current
                };
                let res = match tokio::time::timeout(Duration::from_secs(20), connect).await {
                    Err(_) => None,
                    Ok(net) => net?,
                };
                tracing::debug!("{:?}", res);
                Ok(match res {
                    None => false,
                    Some(net) => &net == ssid,
                })
            }
        }
    }
    #[instrument(skip_all)]
    pub async fn get_current_network(&self) -> Result<Option<Ssid>, Error> {
        let r = Command::new("iwgetid")
            .arg(self.interface.as_str())
            .arg("--raw")
            .invoke(ErrorKind::Wifi)
            .await?;
        let output = String::from_utf8(r)?;
        let network = output.trim();
        tracing::debug!("Current Network: \"{}\"", network);
        if network.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Ssid(network.to_owned())))
        }
    }
    #[instrument(skip_all)]
    pub async fn remove_network(
        &mut self,
        db: TypedPatchDb<Database>,
        ssid: &Ssid,
    ) -> Result<bool, Error> {
        let found_networks = self.find_networks(ssid).await?;
        if found_networks.is_empty() {
            return Ok(true);
        }
        for network_id in found_networks {
            self.remove_network_low(network_id).await?;
        }
        self.save_config(db).await?;
        Ok(true)
    }
    #[instrument(skip_all)]
    pub async fn set_add_network(
        &mut self,
        db: TypedPatchDb<Database>,
        ssid: &Ssid,
        psk: &Psk,
    ) -> Result<(), Error> {
        self.set_add_network_low(ssid, psk).await?;
        self.save_config(db).await?;
        Ok(())
    }
    #[instrument(skip_all)]
    pub async fn add_network(
        &mut self,
        db: TypedPatchDb<Database>,
        ssid: &Ssid,
        psk: &Psk,
    ) -> Result<(), Error> {
        self.add_network_low(ssid, psk).await?;
        self.save_config(db).await?;
        Ok(())
    }
}

#[instrument(skip_all)]
pub async fn interface_connected(interface: &str) -> Result<bool, Error> {
    let out = Command::new("ifconfig")
        .arg(interface)
        .invoke(ErrorKind::Wifi)
        .await?;
    let v = std::str::from_utf8(&out)?
        .lines()
        .find(|s| s.contains("inet"));
    Ok(v.is_some())
}

#[derive(Clone)]
struct CountryCodeParser;
impl TypedValueParser for CountryCodeParser {
    type Value = CountryCode;
    fn parse_ref(
        &self,
        _: &clap::Command,
        _: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let code = value.to_string_lossy();
        CountryCode::for_alpha2(&code).map_err(|_| {
            clap::Error::raw(
                clap::error::ErrorKind::ValueValidation,
                color_eyre::eyre::eyre!("Invalid Country Code: {}", code),
            )
        })
    }
}

#[instrument(skip_all)]
pub async fn synchronize_network_manager<P: AsRef<Path>>(
    main_datadir: P,
    wifi: &WifiInfo,
) -> Result<(), Error> {
    let persistent = main_datadir.as_ref().join("system-connections");

    if tokio::fs::metadata(&persistent).await.is_err() {
        tokio::fs::create_dir_all(&persistent).await?;
    }
    crate::disk::mount::util::bind(&persistent, "/etc/NetworkManager/system-connections", false)
        .await?;

    if !wifi.enabled {
        Command::new("rfkill")
            .arg("block")
            .arg("all")
            .invoke(ErrorKind::Wifi)
            .await?;
    } else {
        Command::new("rfkill")
            .arg("unblock")
            .arg("all")
            .invoke(ErrorKind::Wifi)
            .await?;
    }

    Command::new("ip")
        .arg("rule")
        .arg("add")
        .arg("pref")
        .arg("1000")
        .arg("from")
        .arg("all")
        .arg("lookup")
        .arg("main")
        .invoke(ErrorKind::Network)
        .await
        .log_err();
    Command::new("ip")
        .arg("rule")
        .arg("add")
        .arg("pref")
        .arg("1100")
        .arg("from")
        .arg("all")
        .arg("lookup")
        .arg("default")
        .invoke(ErrorKind::Network)
        .await
        .log_err();

    // IPv6 twins of the two fallbacks above. StartOS owns v6 routing the way it
    // owns v4: `from all lookup main`/`default` sit above NetworkManager's
    // per-tunnel `::/0` full-tunnel rules (~pref 30786), so importing a tunnel
    // that carries an IPv6 address no longer lets NM capture the host's entire
    // IPv6 default route — the v6 default is decided by `main` (by metric),
    // exactly like v4. There is no terminal blackhole: leak prevention is
    // per-gateway instead. A gateway with no IPv6 that is selected as the default
    // outbound points its priority-75 catch-all at a `blackhole default` table
    // (see `apply_policy_routing_v6`), dropping the host's v6 egress without also
    // blackholing replies to inbound tunnel connections — those are pinned to
    // their arrival interface by the priority-50 CONNMARK rule.
    Command::new("ip")
        .arg("-6")
        .arg("rule")
        .arg("add")
        .arg("pref")
        .arg("1000")
        .arg("from")
        .arg("all")
        .arg("lookup")
        .arg("main")
        .invoke(ErrorKind::Network)
        .await
        .log_err();
    Command::new("ip")
        .arg("-6")
        .arg("rule")
        .arg("add")
        .arg("pref")
        .arg("1100")
        .arg("from")
        .arg("all")
        .arg("lookup")
        .arg("default")
        .invoke(ErrorKind::Network)
        .await
        .log_err();

    force_ipv6_addr_gen_eui64().await;

    Command::new("systemctl")
        .arg("restart")
        .arg("NetworkManager")
        .invoke(ErrorKind::Wifi)
        .await?;

    let Some(wifi_iface) = wifi.interface.as_ref().filter(|_| wifi.enabled) else {
        return Ok(());
    };

    Command::new("ifconfig")
        .arg(wifi_iface.as_str())
        .arg("up")
        .invoke(ErrorKind::Wifi)
        .await?;
    if let Some(last_country_code) = wifi.last_region {
        tracing::info!("{}", t!("net.wifi.setting-region"));
        let _ = Command::new("iw")
            .arg("reg")
            .arg("set")
            .arg(last_country_code.alpha2())
            .invoke(ErrorKind::Wifi)
            .await?;
    } else {
        tracing::info!("{}", t!("net.wifi.setting-region-fallback"));
        let _ = Command::new("iw")
            .arg("reg")
            .arg("set")
            .arg("US")
            .invoke(ErrorKind::Wifi)
            .await?;
    }
    Ok(())
}

/// Force EUI-64 IPv6 addressing (privacy extensions off) onto existing physical
/// NetworkManager connections. Connection defaults in NetworkManager.conf only
/// fill in unset properties, so a profile created before those defaults existed
/// keeps its stable-privacy addressing and draws a new address every boot.
async fn force_ipv6_addr_gen_eui64() {
    let Ok(out) = Command::new("nmcli")
        .arg("-t")
        .arg("-f")
        .arg("UUID,TYPE")
        .arg("connection")
        .arg("show")
        .invoke(ErrorKind::Network)
        .await
    else {
        return;
    };
    for line in String::from_utf8_lossy(&out).lines() {
        let mut fields = split_nmcli_terse_line(line).into_iter();
        let (Some(uuid), Some(kind)) = (fields.next(), fields.next()) else {
            continue;
        };
        if kind == "802-3-ethernet" || kind == "802-11-wireless" {
            Command::new("nmcli")
                .arg("connection")
                .arg("modify")
                .arg(&uuid)
                .arg("ipv6.addr-gen-mode")
                .arg("eui64")
                .arg("ipv6.ip6-privacy")
                .arg("0")
                .invoke(ErrorKind::Network)
                .await
                .log_err();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_nmcli_terse_line;

    #[test]
    fn terse_line_unescapes_colon_and_backslash() {
        // nmcli `-t` escapes `:` as `\:` and `\` as `\\` within a field.
        let line = r"my\:wifi\\x:61f7f9e2:802-11-wireless:";
        assert_eq!(
            split_nmcli_terse_line(line),
            vec![
                "my:wifi\\x".to_owned(),
                "61f7f9e2".to_owned(),
                "802-11-wireless".to_owned(),
                String::new(),
            ],
        );
    }

    #[test]
    fn terse_line_plain_fields() {
        assert_eq!(
            split_nmcli_terse_line("café:90:WPA2 WPA3"),
            vec!["café".to_owned(), "90".to_owned(), "WPA2 WPA3".to_owned()],
        );
    }
}
