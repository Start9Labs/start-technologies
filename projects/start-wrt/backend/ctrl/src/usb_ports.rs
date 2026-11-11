//! USB ports as Ethernet ports.
//!
//! A physical USB port becomes a LAN bridge port the first time an Ethernet-type
//! network device is plugged into it. The port is a veth named `usb_eth<N>`: one end
//! joins the LAN bridge like any Ethernet port and carries the port's profile, the
//! other joins `br-usb_eth<N>`, which the network devices of the USB device in that
//! physical port join. A hub's ports are physical ports of their own. The physical
//! port, its name, and its profile persist while nothing is plugged in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use uciedit::openwrt::{
    NetworkBridgeVlan, NetworkDevice, NetworkInterface, NetworkVlanPort, NetworkVlanPortTagging,
};
use uciedit::{dump_all, parse_all, Arena, Configs, TypedSection};

use crate::prelude::*;

pub const PORT_PREFIX: &str = "usb_eth";

/// A physical USB port recorded in `/etc/config/startwrt`, named for its LAN
/// bridge port.
#[derive(Debug, TypedSection)]
#[uci(ty = "usb_port")]
pub struct UciUsbPort {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct UsbPortStatus {
    /// A network device is plugged into the port.
    pub present: bool,
    /// A network device in the port has link.
    pub active: bool,
}
rpc_toolkit::reflect_ts!(UsbPortStatus);

/// Sysfs, rooted elsewhere in tests.
pub struct Sys {
    root: PathBuf,
}

impl Sys {
    pub fn host() -> Self {
        Self::at("/sys")
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn net(&self, ifname: &str) -> PathBuf {
        self.root.join("class/net").join(ifname)
    }

    fn exists(&self, ifname: &str) -> bool {
        self.net(ifname).exists()
    }

    fn master(&self, ifname: &str) -> Option<String> {
        std::fs::read_link(self.net(ifname).join("master"))
            .ok()?
            .file_name()?
            .to_str()
            .map(str::to_owned)
    }

    fn members(&self, bridge: &str) -> Vec<String> {
        std::fs::read_dir(self.net(bridge).join("brif"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect()
    }

    fn carrier(&self, ifname: &str) -> bool {
        read_trim(self.net(ifname).join("carrier")).as_deref() == Some("1")
    }

    /// The USB device a network device belongs to.
    fn usb_device(&self, ifname: &str) -> Option<PathBuf> {
        let interface = std::fs::canonicalize(self.net(ifname).join("device")).ok()?;
        let device = interface.parent()?;
        device
            .join("authorized")
            .exists()
            .then(|| device.to_owned())
    }

    /// The physical USB port an Ethernet-type network device is plugged into:
    /// its controller and the port chain on the port's USB 2 side.
    pub fn physical_port(&self, ifname: &str) -> Option<String> {
        let net = self.net(ifname);
        if !is_ethernet(&net) {
            return None;
        }
        let device = std::fs::canonicalize(net.join("device")).ok()?;
        let devices = std::fs::canonicalize(self.root.join("devices")).ok()?;
        let parts: Vec<&str> = device
            .strip_prefix(&devices)
            .ok()?
            .iter()
            .map(|c| c.to_str())
            .collect::<Option<_>>()?;
        let hub = parts.iter().rposition(|p| is_root_hub(p))?;
        let chain: Vec<&str> = parts[hub + 1..]
            .iter()
            .copied()
            .take_while(|p| is_usb_device(p))
            .collect();
        if chain.is_empty() {
            return None;
        }
        let root_hub: PathBuf = devices.join(parts[..=hub].iter().collect::<PathBuf>());
        let usb_device = root_hub.join(chain.iter().collect::<PathBuf>());
        let mut port = port_node(&usb_device)?;
        let mut side = "";
        if read_trim(root_hub.join("speed")).and_then(|s| s.parse::<u32>().ok()) >= Some(5000) {
            match std::fs::canonicalize(port.join("peer")) {
                Ok(peer) => port = peer,
                Err(_) => side = "ss:",
            }
        }
        let chain = port_chain(port.file_name()?.to_str()?)?;
        let controller: Vec<&str> = parts[..hub]
            .iter()
            .copied()
            .filter(|p| !p.ends_with(".auto"))
            .collect();
        Some(format!("{}:{side}{chain}", controller.join("/")))
    }
}

/// The hub port node a USB device sits in, present before the device's own `port` link.
fn port_node(usb_device: &Path) -> Option<PathBuf> {
    let name = usb_device.file_name()?.to_str()?;
    let n = name.rsplit(['.', '-']).next()?;
    let hub_dir = usb_device.parent()?;
    let hub = hub_dir.file_name()?.to_str()?;
    let node = match hub.strip_prefix("usb").filter(|_| is_root_hub(hub)) {
        Some(bus) => format!("{bus}-0:1.0/{hub}-port{n}"),
        None => format!("{hub}:1.0/{hub}-port{n}"),
    };
    std::fs::canonicalize(hub_dir.join(node)).ok()
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| {
        s.trim_matches(|c: char| c.is_whitespace() || c == '\0')
            .to_owned()
    })
}

/// Wireless, cellular, and virtual devices are not ports.
fn is_ethernet(net: &Path) -> bool {
    read_trim(net.join("type")).as_deref() == Some("1")
        && !net.join("wireless").exists()
        && !net.join("phy80211").exists()
        && read_trim(net.join("uevent"))
            .is_some_and(|uevent| !uevent.lines().any(|l| l.starts_with("DEVTYPE=")))
}

fn is_root_hub(name: &str) -> bool {
    name.strip_prefix("usb")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `<bus>-<port>[.<port>…]`
fn is_usb_device(name: &str) -> bool {
    name.split_once('-').is_some_and(|(bus, path)| {
        !bus.is_empty()
            && bus.bytes().all(|b| b.is_ascii_digit())
            && path
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// The dotted port chain a sysfs port node names: `usb2-port1` is `1`, `2-1.3-port2` is `1.3.2`.
fn port_chain(port: &str) -> Option<String> {
    let (hub, n) = port.rsplit_once("-port")?;
    n.parse::<u32>().ok()?;
    if is_root_hub(hub) {
        Some(n.to_owned())
    } else {
        let (_, path) = hub.split_once('-')?;
        Some(format!("{path}.{n}"))
    }
}

/// The recorded ports, by name.
pub fn recorded(cfgs: &Configs) -> Result<BTreeMap<String, String>, Error> {
    let mut ports = BTreeMap::new();
    cfgs["startwrt"].try_each(|name, port: UciUsbPort| {
        if let Some(name) = name {
            ports.insert(name.to_owned(), port.path);
        }
        Ok::<_, Error>(())
    })?;
    Ok(ports)
}

pub fn statuses(sys: &Sys, cfgs: &Configs) -> Result<BTreeMap<String, UsbPortStatus>, Error> {
    Ok(recorded(cfgs)?
        .into_keys()
        .filter_map(|name| {
            let uplink = uplink(&name);
            let devices: Vec<String> = sys
                .members(&bridge(&name))
                .into_iter()
                .filter(|m| *m != uplink)
                .collect();
            Some((
                name,
                UsbPortStatus {
                    present: !devices.is_empty(),
                    active: devices.iter().any(|m| sys.carrier(m)),
                },
            ))
        })
        .collect())
}

/// What a profile change takes down and up to make a port's clients lose link.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Interface(String),
    /// A USB adapter keeps link with its interface down.
    UsbDevice(PathBuf),
}

impl LinkTarget {
    pub async fn set_link(&self, up: bool) {
        match self {
            Self::Interface(ifname) => {
                let _ = ip(&["link", "set", ifname, if up { "up" } else { "down" }]).await;
            }
            Self::UsbDevice(device) => {
                let _ = std::fs::write(device.join("authorized"), if up { "1" } else { "0" });
            }
        }
    }
}

/// The devices in a USB port, or the port itself.
pub fn link_targets(sys: &Sys, port: &str) -> Vec<LinkTarget> {
    if !port.starts_with(PORT_PREFIX) || !sys.exists(&bridge(port)) {
        return vec![LinkTarget::Interface(port.to_owned())];
    }
    let uplink = uplink(port);
    let mut targets = Vec::new();
    for member in sys.members(&bridge(port)) {
        if member == uplink {
            continue;
        }
        let target = match sys.usb_device(&member) {
            Some(device) => LinkTarget::UsbDevice(device),
            None => LinkTarget::Interface(member),
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

fn bridge(port: &str) -> String {
    format!("br-{port}")
}

fn uplink(port: &str) -> String {
    format!("{port}-up")
}

/// The port whose uplink this interface is.
fn uplink_port(ifname: &str) -> Option<&str> {
    ifname
        .strip_suffix("-up")
        .filter(|port| port.starts_with(PORT_PREFIX))
}

/// Device names netifd configures.
fn netifd_names(cfgs: &Configs) -> Result<BTreeSet<String>, Error> {
    let mut names = BTreeSet::new();
    cfgs["network"].try_each(|_, dev: NetworkDevice| {
        names.insert(dev.name);
        names.extend(dev.ports);
        Ok::<_, Error>(())
    })?;
    cfgs["network"].try_each(|_, iface: NetworkInterface| {
        names.insert(iface.device);
        Ok::<_, Error>(())
    })?;
    Ok(names)
}

#[derive(Debug, PartialEq, Eq)]
struct Registered {
    name: String,
    first_seen: bool,
    joined_bridge: bool,
}

/// Records the physical port and puts its port on the LAN bridge, in the Admin
/// profile while VLAN filtering is on.
fn register_config(cfgs: &mut Configs, path: &str) -> Result<Registered, Error> {
    let recorded = recorded(cfgs)?;
    let lan = crate::ethernet::find_lan_bridge(cfgs)?
        .ok_or_else(|| Error::new(eyre!("missing LAN bridge"), ErrorKind::MissingLanBridge))?;
    let existing = recorded
        .iter()
        .find(|(_, p)| *p == path)
        .map(|(name, _)| name.clone());
    let first_seen = existing.is_none();
    let name = match existing {
        Some(name) => name,
        None => {
            let name = (0..)
                .map(|n| format!("{PORT_PREFIX}{n}"))
                .find(|n| !recorded.contains_key(n) && !lan.ports.contains(n))
                .unwrap_or_default();
            cfgs["startwrt"].append(
                &UciUsbPort {
                    path: path.to_owned(),
                },
                Some(name.as_str()),
            )?;
            name
        }
    };
    let joined_bridge = !lan.ports.contains(&name);
    if joined_bridge {
        for section in &mut cfgs["network"].sections {
            if let Some(mut dev) = section.get_typed::<NetworkDevice>()? {
                if dev.name == lan.name {
                    dev.ports.push(name.clone());
                    section.set(&dev)?;
                }
            }
            if let Some(mut vlan) = section.get_typed::<NetworkBridgeVlan>()? {
                if vlan.device == lan.name
                    && vlan.vlan == 1
                    && !vlan.ports.iter().any(|p| p.port == name)
                {
                    vlan.ports.push(NetworkVlanPort {
                        port: name.clone(),
                        tagging: Some(NetworkVlanPortTagging::PRIMARY),
                    });
                    section.set(&vlan)?;
                }
            }
        }
    }
    Ok(Registered {
        name,
        first_seen,
        joined_bridge,
    })
}

async fn register(uci_root: &Path, path: &str) -> Result<Registered, Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(uci_root, &arena, &["network", "startwrt"]).await?;
        let registered = register_config(&mut cfgs, path)?;
        if !registered.first_seen && !registered.joined_bridge {
            return Ok(registered);
        }
        match dump_all(uci_root, cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => retries -= 1,
            Err(e) => return Err(e.into()),
            Ok(()) => return Ok(registered),
        }
    }
}

/// Takes the port off the LAN bridge and forgets its physical port.
fn remove_config(cfgs: &mut Configs, port: &str) -> Result<(), Error> {
    if !recorded(cfgs)?.contains_key(port) {
        return Err(Error::new(
            eyre!("No such USB port: {port}"),
            ErrorKind::NotFound,
        ));
    }
    for section in &mut cfgs["network"].sections {
        if let Some(mut dev) = section.get_typed::<NetworkDevice>()? {
            if dev.ports.iter().any(|p| p == port) {
                dev.ports.retain(|p| p != port);
                section.set(&dev)?;
            }
        }
        if let Some(mut vlan) = section.get_typed::<NetworkBridgeVlan>()? {
            if vlan.ports.iter().any(|p| p.port == port) {
                vlan.ports.retain(|p| p.port != port);
                section.set(&vlan)?;
            }
        }
    }
    cfgs["startwrt"].sections.retain(|section| {
        section.name().as_deref() != Some(port) || section.get::<UciUsbPort>().is_err()
    });
    Ok(())
}

/// Removes a port with no network device plugged into it.
pub async fn remove(sys: &Sys, uci_root: &Path, port: &str, effectful: bool) -> Result<(), Error> {
    let uplink = uplink(port);
    let bridge = bridge(port);
    if sys.members(&bridge).iter().any(|m| *m != uplink) {
        return Err(Error::new(
            eyre!("Unplug the network device from {port} before removing it"),
            ErrorKind::InvalidValue,
        ));
    }
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(uci_root, &arena, &["network", "startwrt"]).await?;
        remove_config(&mut cfgs, port)?;
        match dump_all(uci_root, cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => retries -= 1,
            Err(e) => return Err(e.into()),
            Ok(()) => break,
        }
    }
    if effectful {
        for ifname in [port, bridge.as_str()] {
            if sys.exists(ifname) {
                ip(&["link", "del", ifname]).await?;
            }
        }
    }
    Ok(())
}

async fn command(program: &str, args: &[&str]) -> Result<(), Error> {
    let status = crate::run_quiet_async(tokio::process::Command::new(program).args(args)).await?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            eyre!("{program} {} failed: {status}", args.join(" ")),
            ErrorKind::Network,
        ))
    }
}

async fn ip(args: &[&str]) -> Result<(), Error> {
    command("ip", args).await
}

/// Bridge members and bridges carry no addresses of their own.
fn disable_ipv6(ifname: &str) {
    let _ = std::fs::write(
        format!("/proc/sys/net/ipv6/conf/{ifname}/disable_ipv6"),
        "1",
    );
}

/// Creates the port's veth and bridge when missing and brings up all but the
/// LAN bridge side.
async fn ensure_links(sys: &Sys, port: &str) -> Result<(), Error> {
    let bridge = bridge(port);
    let uplink = uplink(port);
    if !sys.exists(&bridge) {
        ip(&[
            "link",
            "add",
            bridge.as_str(),
            "type",
            "bridge",
            "mcast_snooping",
            "0",
        ])
        .await?;
    }
    disable_ipv6(&bridge);
    if !sys.exists(port) {
        ip(&[
            "link",
            "add",
            port,
            "type",
            "veth",
            "peer",
            "name",
            uplink.as_str(),
        ])
        .await?;
    }
    disable_ipv6(port);
    disable_ipv6(&uplink);
    if sys.master(&uplink).as_deref() != Some(bridge.as_str()) {
        ip(&["link", "set", uplink.as_str(), "master", bridge.as_str()]).await?;
    }
    for ifname in [uplink.as_str(), bridge.as_str()] {
        ip(&["link", "set", ifname, "up"]).await?;
    }
    Ok(())
}

/// Joins the port to the LAN bridge in its configured VLAN; it comes up only
/// once that VLAN is set.
async fn join_lan(uci_root: &Path, port: &str) -> Result<(), Error> {
    let vlan = {
        let arena = Arena::new();
        let cfgs = parse_all(uci_root, &arena, &["network"]).await?;
        crate::ethernet::port_vlans(&cfgs)?.get(port).copied()
    };
    ip(&["link", "set", port, "down"]).await?;
    ip(&[
        "link",
        "set",
        port,
        "master",
        crate::ethernet::DEFAULT_LAN_BRIDGE,
    ])
    .await?;
    if let Some(vlan) = vlan {
        let vid = vlan.to_string();
        command(
            "bridge",
            &[
                "vlan",
                "add",
                "dev",
                port,
                "vid",
                vid.as_str(),
                "pvid",
                "untagged",
            ],
        )
        .await?;
        if vlan != 1 {
            let _ = command("bridge", &["vlan", "del", "dev", port, "vid", "1"]).await;
        }
    }
    ip(&["link", "set", port, "up"]).await
}

async fn handle(sys: &Sys, uci_root: &Path, ifname: &str) -> Result<(), Error> {
    if let Some(port) = uplink_port(ifname) {
        // netifd takes a veth's peer for its parent and resets the peer, IPv6
        // last, when it claims the veth.
        disable_ipv6(ifname);
        if sys.master(ifname).as_deref() != Some(bridge(port).as_str()) {
            let arena = Arena::new();
            let cfgs = parse_all(uci_root, &arena, &["startwrt"]).await?;
            if recorded(&cfgs)?.contains_key(port) {
                ensure_links(sys, port).await?;
            }
        }
        return Ok(());
    }
    if sys
        .master(ifname)
        .is_some_and(|m| m.starts_with(&bridge(PORT_PREFIX)))
    {
        return Ok(());
    }
    let Some(path) = sys.physical_port(ifname) else {
        if let Some(device) = sys
            .usb_device(ifname)
            .filter(|_| is_ethernet(&sys.net(ifname)))
        {
            tracing::warn!(
                "usb-ports: {ifname}: no physical port for {}",
                device.display()
            );
        }
        return Ok(());
    };
    {
        let arena = Arena::new();
        let cfgs = parse_all(uci_root, &arena, &["network"]).await?;
        if netifd_names(&cfgs)?.contains(ifname) {
            tracing::warn!("usb-ports: {ifname} has a name netifd configures; leaving it alone");
            return Ok(());
        }
    }
    let registered = register(uci_root, &path).await?;
    let port = registered.name.as_str();
    ensure_links(sys, port).await?;
    if sys.master(port).as_deref() != Some(crate::ethernet::DEFAULT_LAN_BRIDGE) {
        join_lan(uci_root, port).await?;
    }
    disable_ipv6(ifname);
    let port_bridge = bridge(port);
    ip(&["link", "set", ifname, "master", port_bridge.as_str()]).await?;
    ip(&["link", "set", ifname, "up"]).await?;
    // Packet steering otherwise reaches a hot-plugged interface at the next network reload.
    if let Err(e) = command("/etc/init.d/packet_steering", &["reload"]).await {
        tracing::warn!("usb-ports: {ifname}: {e}");
    }
    if registered.first_seen {
        crate::activity::log(
            "ethernet",
            "usb-port-added",
            true,
            &format!("Added {port} when a network device was plugged into its USB port"),
            None,
        );
    }
    Ok(())
}

async fn coldplug(sys: &Sys, uci_root: &Path) {
    let ports = async {
        let arena = Arena::new();
        let cfgs = parse_all(uci_root, &arena, &["startwrt"]).await?;
        recorded(&cfgs)
    }
    .await;
    match ports {
        Ok(ports) => {
            for port in ports.keys() {
                if let Err(e) = ensure_links(sys, port).await {
                    tracing::warn!("usb-ports: {port}: {e}");
                }
            }
        }
        Err(e) => tracing::warn!("usb-ports: reading recorded ports: {e}"),
    }
    let names: Vec<String> = std::fs::read_dir(sys.root.join("class/net"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    for ifname in names {
        if let Err(e) = handle(sys, uci_root, &ifname).await {
            tracing::warn!("usb-ports: {ifname}: {e}");
        }
    }
}

/// The interface a `ip -o monitor link` line reports as added or changed.
fn new_link(line: &str) -> Option<&str> {
    if line.starts_with("Deleted") {
        return None;
    }
    let (_, rest) = line.split_once(": ")?;
    let (name, _) = rest.split_once(": ")?;
    name.split('@').next().filter(|n| !n.is_empty())
}

/// Follows link events for the daemon's lifetime.
pub async fn run() {
    let sys = Sys::host();
    let uci_root = Path::new("/etc/config");
    let mut backoff = 5u64;
    loop {
        let mut child = match tokio::process::Command::new("ip")
            .args(["-o", "monitor", "link"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                tracing::error!("usb-ports: failed to spawn ip monitor: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
                continue;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            continue;
        };
        let mut lines = BufReader::new(stdout).lines();
        coldplug(&sys, uci_root).await;
        backoff = 5;
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(ifname) = new_link(&line) {
                if let Err(e) = handle(&sys, uci_root, ifname).await {
                    tracing::warn!("usb-ports: {ifname}: {e}");
                }
            }
        }
        tracing::warn!("usb-ports: ip monitor exited; respawning in {backoff}s");
        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    const DWC3: &str = "devices/platform/soc/d4282bc8.usb3/c0a00000.dwc3";
    const KEY: &str = "platform/soc/d4282bc8.usb3/c0a00000.dwc3";

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        /// An xHCI controller with a USB 2 and a USB 3 root hub, one port each, peered.
        fn new() -> Self {
            let fx = Fixture {
                dir: tempfile::tempdir().unwrap(),
            };
            let host = fx.path(&format!("{DWC3}/xhci-hcd.0.auto"));
            for (bus, speed) in [("2", "480"), ("3", "5000")] {
                let hub = host.join(format!("usb{bus}"));
                std::fs::create_dir_all(hub.join(format!("{bus}-0:1.0/usb{bus}-port1"))).unwrap();
                std::fs::write(hub.join("speed"), speed).unwrap();
            }
            fx.peer(
                &host.join("usb2/2-0:1.0/usb2-port1"),
                &host.join("usb3/3-0:1.0/usb3-port1"),
            );
            std::fs::create_dir_all(fx.path("class/net")).unwrap();
            fx
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.path().join("sys").join(rel)
        }

        fn sys(&self) -> Sys {
            Sys::at(self.dir.path().join("sys"))
        }

        fn host(&self) -> PathBuf {
            self.path(&format!("{DWC3}/xhci-hcd.0.auto"))
        }

        fn peer(&self, a: &Path, b: &Path) {
            symlink(b, a.join("peer")).unwrap();
            symlink(a, b.join("peer")).unwrap();
        }

        /// A USB device in a port, with `ports` downstream ports if it is a hub.
        fn device(&self, bus: &str, devpath: &str, ports: u32) -> PathBuf {
            let parent = devpath.rsplit_once('.').map(|(parent, _)| parent);
            let hub = self.host().join(format!("usb{bus}"));
            let dir = match parent {
                None => hub.join(format!("{bus}-{devpath}")),
                Some(parent) => self
                    .device_dir(bus, parent)
                    .join(format!("{bus}-{devpath}")),
            };
            std::fs::create_dir_all(&dir).unwrap();
            for p in 1..=ports {
                std::fs::create_dir_all(
                    dir.join(format!("{bus}-{devpath}:1.0/{bus}-{devpath}-port{p}")),
                )
                .unwrap();
            }
            dir
        }

        fn device_dir(&self, bus: &str, devpath: &str) -> PathBuf {
            let hub = self.host().join(format!("usb{bus}"));
            let mut dir = hub;
            let mut prefix = String::new();
            for part in devpath.split('.') {
                if !prefix.is_empty() {
                    prefix.push('.');
                }
                prefix.push_str(part);
                dir = dir.join(format!("{bus}-{prefix}"));
            }
            dir
        }

        /// A hub on both buses with peered ports.
        fn hub(&self, devpath: &str, ports: u32) {
            self.device("2", devpath, ports);
            self.device("3", devpath, ports);
            for p in 1..=ports {
                let port = |bus: &str| {
                    self.device_dir(bus, devpath)
                        .join(format!("{bus}-{devpath}:1.0/{bus}-{devpath}-port{p}"))
                };
                self.peer(&port("2"), &port("3"));
            }
        }

        fn netdev(&self, ifname: &str, device: &Path, uevent: &str) {
            let iface = device.join(format!(
                "{}:1.0",
                device.file_name().unwrap().to_str().unwrap()
            ));
            std::fs::create_dir_all(&iface).unwrap();
            let net = self.path("class/net").join(ifname);
            std::fs::create_dir_all(&net).unwrap();
            std::fs::write(net.join("type"), "1").unwrap();
            std::fs::write(net.join("uevent"), uevent).unwrap();
            symlink(&iface, net.join("device")).unwrap();
        }
    }

    #[test]
    fn each_socket_is_its_own_port() {
        let fx = Fixture::new();
        fx.netdev("eth2", &fx.device("3", "1", 0), "");
        assert_eq!(
            fx.sys().physical_port("eth2").as_deref(),
            Some(format!("{KEY}:1").as_str())
        );

        let fx = Fixture::new();
        fx.hub("1", 4);
        fx.hub("1.2", 4);
        fx.netdev(
            "eth2",
            &fx.device("3", "1.4", 0),
            "INTERFACE=eth2\nIFINDEX=7\n",
        );
        fx.netdev("eth3", &fx.device("3", "1.2.1", 0), "");
        fx.netdev("eth4", &fx.device("2", "1.2.4", 0), "");
        let sys = fx.sys();
        for (ifname, chain) in [("eth2", "1.4"), ("eth3", "1.2.1"), ("eth4", "1.2.4")] {
            assert_eq!(
                sys.physical_port(ifname).as_deref(),
                Some(format!("{KEY}:{chain}").as_str()),
                "{ifname}"
            );
        }
    }

    #[test]
    fn usb2_and_usb3_adapters_in_one_port_match() {
        let fx = Fixture::new();
        fx.hub("1", 4);
        fx.netdev("eth2", &fx.device("3", "1.3", 0), "");
        fx.netdev("eth3", &fx.device("2", "1.3", 0), "");
        let sys = fx.sys();
        assert_eq!(sys.physical_port("eth2"), sys.physical_port("eth3"));
    }

    #[test]
    fn other_devices_are_not_ports() {
        let fx = Fixture::new();
        fx.hub("1", 4);
        fx.netdev("wwan0", &fx.device("3", "1.1", 0), "DEVTYPE=wwan\n");
        fx.netdev("wlan1", &fx.device("3", "1.2", 0), "DEVTYPE=wlan\n");
        let raw = fx.device("3", "1.3", 0);
        fx.netdev("wwan1", &raw, "");
        std::fs::write(fx.path("class/net/wwan1/type"), "65534").unwrap();
        let soc = fx.path("devices/platform/soc/cac80000.ethernet");
        std::fs::create_dir_all(&soc).unwrap();
        let net = fx.path("class/net/eth0");
        std::fs::create_dir_all(&net).unwrap();
        std::fs::write(net.join("type"), "1").unwrap();
        std::fs::write(net.join("uevent"), "").unwrap();
        symlink(&soc, net.join("device")).unwrap();
        let virt = fx.path("class/net/usb_eth0");
        std::fs::create_dir_all(&virt).unwrap();
        std::fs::write(virt.join("type"), "1").unwrap();
        std::fs::write(virt.join("uevent"), "").unwrap();
        let sys = fx.sys();
        for ifname in ["wwan0", "wlan1", "wwan1", "eth0", "usb_eth0", "missing"] {
            assert_eq!(sys.physical_port(ifname), None, "{ifname}");
        }
    }

    #[test]
    fn port_chains() {
        assert_eq!(port_chain("usb2-port1").as_deref(), Some("1"));
        assert_eq!(port_chain("2-1-port4").as_deref(), Some("1.4"));
        assert_eq!(port_chain("2-1.3-port2").as_deref(), Some("1.3.2"));
        assert_eq!(port_chain("2-1:1.0"), None);
    }

    #[test]
    fn uplink_names() {
        assert_eq!(uplink_port("usb_eth0-up"), Some("usb_eth0"));
        assert_eq!(uplink_port(&uplink("usb_eth11")), Some("usb_eth11"));
        assert_eq!(uplink_port("usb_eth0"), None);
        assert_eq!(uplink_port("eth2-up"), None);
    }

    #[test]
    fn monitor_lines() {
        assert_eq!(
            new_link("7: eth2: <BROADCAST,MULTICAST> mtu 1500 qdisc noop state DOWN \\    link/ether 00:e0:4c:68:00:01 brd ff:ff:ff:ff:ff:ff"),
            Some("eth2")
        );
        assert_eq!(
            new_link("9: usb_eth0@usb_eth0-up: <BROADCAST,MULTICAST,M-DOWN> mtu 1500"),
            Some("usb_eth0")
        );
        assert_eq!(
            new_link("Deleted 7: eth2: <BROADCAST,MULTICAST> mtu 1500"),
            None
        );
        assert_eq!(new_link(""), None);
    }

    #[tokio::test]
    async fn removing_a_port_forgets_it() {
        let fx = Fixture::new();
        let dir = tempfile::tempdir().unwrap();
        configs(dir.path(), FILTERING, "");
        register(dir.path(), "c:1.4").await.unwrap();
        register(dir.path(), "c:1.2").await.unwrap();
        remove(&fx.sys(), dir.path(), "usb_eth0", false)
            .await
            .unwrap();
        let network = std::fs::read_to_string(dir.path().join("network")).unwrap();
        assert!(!network.contains("usb_eth0"), "{network}");
        assert_eq!(network.matches("usb_eth1").count(), 2, "{network}");
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        assert_eq!(
            recorded(&cfgs).unwrap(),
            BTreeMap::from([("usb_eth1".to_owned(), "c:1.2".to_owned())])
        );
        assert!(remove(&fx.sys(), dir.path(), "usb_eth0", false)
            .await
            .is_err());
        assert_eq!(
            register(dir.path(), "c:1.3").await.unwrap().name,
            "usb_eth0"
        );
    }

    #[tokio::test]
    async fn a_port_with_a_device_plugged_in_stays() {
        let fx = Fixture::new();
        let brif = fx.path("class/net/br-usb_eth0/brif");
        std::fs::create_dir_all(&brif).unwrap();
        std::fs::write(brif.join("usb_eth0-up"), "").unwrap();
        std::fs::write(brif.join("eth5"), "").unwrap();
        let dir = tempfile::tempdir().unwrap();
        configs(dir.path(), FLAT, "");
        register(dir.path(), "c:1.4").await.unwrap();
        let before = std::fs::read_to_string(dir.path().join("network")).unwrap();
        let err = remove(&fx.sys(), dir.path(), "usb_eth0", false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unplug"), "{err}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("network")).unwrap(),
            before
        );
    }

    fn configs(dir: &Path, network: &str, startwrt: &str) {
        std::fs::write(dir.join("network"), network).unwrap();
        std::fs::write(dir.join("startwrt"), startwrt).unwrap();
    }

    const FLAT: &str = "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
";

    const FILTERING: &str = "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '20'
";

    #[tokio::test]
    async fn first_sight_records_the_port_in_admin() {
        let dir = tempfile::tempdir().unwrap();
        configs(dir.path(), FILTERING, "");
        let reg = register(dir.path(), "c:1.4").await.unwrap();
        assert_eq!(
            reg,
            Registered {
                name: "usb_eth0".into(),
                first_seen: true,
                joined_bridge: true,
            }
        );
        let network = std::fs::read_to_string(dir.path().join("network")).unwrap();
        assert!(network.contains("list ports 'usb_eth0'\n"), "{network}");
        assert!(network.contains("list ports 'usb_eth0:u*'\n"), "{network}");
        assert_eq!(network.matches("usb_eth0").count(), 2, "{network}");
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        assert_eq!(
            recorded(&cfgs).unwrap(),
            BTreeMap::from([("usb_eth0".to_owned(), "c:1.4".to_owned())])
        );

        let again = register(dir.path(), "c:1.4").await.unwrap();
        assert_eq!(
            again,
            Registered {
                name: "usb_eth0".into(),
                first_seen: false,
                joined_bridge: false,
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("network")).unwrap(),
            network
        );
    }

    #[tokio::test]
    async fn a_flat_bridge_gets_no_vlan_entry() {
        let dir = tempfile::tempdir().unwrap();
        configs(dir.path(), FLAT, "");
        register(dir.path(), "c:1").await.unwrap();
        let network = std::fs::read_to_string(dir.path().join("network")).unwrap();
        assert!(network.contains("list ports 'usb_eth0'\n"), "{network}");
        assert!(!network.contains("bridge-vlan"), "{network}");
    }

    #[tokio::test]
    async fn names_skip_recorded_and_bridged_ports() {
        let dir = tempfile::tempdir().unwrap();
        configs(
            dir.path(),
            &FLAT.replace(
                "\tlist ports 'eth0'\n",
                "\tlist ports 'eth0'\n\tlist ports 'usb_eth1'\n",
            ),
            "config usb_port 'usb_eth0'\n\toption path 'c:1.1'\n",
        );
        let reg = register(dir.path(), "c:1.3").await.unwrap();
        assert_eq!(reg.name, "usb_eth2");
        let reg = register(dir.path(), "c:1.1").await.unwrap();
        assert_eq!(reg.name, "usb_eth0");
        assert!(!reg.first_seen);
        assert!(reg.joined_bridge);
    }

    #[tokio::test]
    async fn netifd_names_cover_bridges_ports_and_interfaces() {
        let dir = tempfile::tempdir().unwrap();
        let network = "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth2'

config interface 'wan'
\toption device 'eth1'
\toption proto 'dhcp'
";
        configs(dir.path(), network, "");
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network"]).await.unwrap();
        let names = netifd_names(&cfgs).unwrap();
        for name in ["br-lan", "eth0", "eth1", "eth2"] {
            assert!(names.contains(name), "{name}");
        }
        assert!(!names.contains("eth5"));
    }

    #[tokio::test]
    async fn statuses_report_link_below_each_port() {
        let fx = Fixture::new();
        for (ifname, master, carrier) in [
            ("usb_eth0-up", "br-usb_eth0", "1"),
            ("eth5", "br-usb_eth0", "1"),
            ("usb_eth1-up", "br-usb_eth1", "1"),
            ("eth6", "br-usb_eth1", "0"),
        ] {
            let net = fx.path("class/net").join(ifname);
            std::fs::create_dir_all(&net).unwrap();
            std::fs::write(net.join("carrier"), carrier).unwrap();
            let brif = fx.path("class/net").join(master).join("brif");
            std::fs::create_dir_all(&brif).unwrap();
            std::fs::write(brif.join(ifname), "").unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        configs(
            dir.path(),
            FLAT,
            "config usb_port 'usb_eth0'\n\toption path 'c:1.1'\n\nconfig usb_port 'usb_eth1'\n\toption path 'c:1.2'\n\nconfig usb_port 'usb_eth2'\n\toption path 'c:1.3'\n",
        );
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        let statuses = statuses(&fx.sys(), &cfgs).unwrap();
        let summary: Vec<(&str, bool, bool)> = statuses
            .iter()
            .map(|(name, s)| (name.as_str(), s.present, s.active))
            .collect();
        assert_eq!(
            summary,
            [
                ("usb_eth0", true, true),
                ("usb_eth1", true, false),
                ("usb_eth2", false, false)
            ]
        );
        let interface = |name: &str| vec![LinkTarget::Interface(name.to_owned())];
        assert_eq!(link_targets(&fx.sys(), "usb_eth1"), interface("eth6"));
        assert_eq!(link_targets(&fx.sys(), "usb_eth2"), interface("usb_eth2"));
        assert_eq!(link_targets(&fx.sys(), "eth0"), interface("eth0"));

        let adapter = fx.device("3", "1", 0);
        std::fs::write(adapter.join("authorized"), "1").unwrap();
        fx.netdev("eth7", &adapter, "");
        std::fs::write(fx.path("class/net/br-usb_eth1/brif/eth7"), "").unwrap();
        let targets = link_targets(&fx.sys(), "usb_eth1");
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&LinkTarget::Interface("eth6".to_owned())));
        assert!(targets.contains(&LinkTarget::UsbDevice(
            std::fs::canonicalize(&adapter).unwrap()
        )));
    }
}
