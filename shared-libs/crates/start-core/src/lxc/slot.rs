//! A container slot is the index an installed service holds for its whole installation. It is the
//! single owner of everything that distinguishes one service container from another on the host:
//! its addresses on the container network, the host id range its namespace maps, and the host
//! uid that owns that namespace. Slots are persisted on the package entry, assigned when a package
//! is first installed and freed only by uninstall.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::{Ipv4Net, Ipv6Net};
use serde::{Deserialize, Serialize};

use crate::db::model::package::AllPackageData;
use crate::disk::mount::filesystem::idmapped::IdMap;
use crate::prelude::*;

/// Installed services one server holds.
pub const MAX_SERVICES: u16 = 1024;

/// Ids one container maps, and the distance between consecutive containers' host ranges.
pub const ID_RANGE: u32 = 1 << 16;

/// The server on the container network: forwards, the TLS proxy, DNS and Tor.
pub const HOST_IP: Ipv4Addr = Ipv4Addr::new(172, 16, 0, 1);
pub const HOST_IPV6: Ipv6Addr = Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 1);

/// The server's own listeners that forwards and the TLS proxy target, such as the StartOS UI.
pub const OS_LISTEN_IP: Ipv4Addr = Ipv4Addr::new(172, 16, 0, 2);
pub const OS_LISTEN_IPV6: Ipv6Addr = Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 2);

/// Whether the address is one of the server's own listen addresses. Never an operator-visible
/// address: interface observation omits it.
pub fn is_os_listen(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => ip == OS_LISTEN_IP,
        IpAddr::V6(ip) => ip == OS_LISTEN_IPV6,
    }
}

/// Every address a container or the server holds on the container network.
pub const CONTAINER_NET: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(172, 16, 0, 0), 16);
pub const CONTAINER_NET_V6: Ipv6Net =
    Ipv6Net::new_assert(Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 0), 64);

/// Host-side name prefix of every container's veth.
pub const VETH_PREFIX: &str = "lxc-svc-";

const FIRST_CONTAINER_HOST: u32 = 3;

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash, VisitFields,
)]
#[visit(ts(type = "z.number()"))]
pub struct ContainerSlot(u16);

rpc_toolkit::reflect_ts!(ContainerSlot);

impl ContainerSlot {
    pub fn new(index: u16) -> Result<Self, Error> {
        if index < MAX_SERVICES {
            Ok(Self(index))
        } else {
            Err(service_limit())
        }
    }

    /// The lowest slot no installed package holds.
    pub fn lowest_free(used: impl IntoIterator<Item = Self>) -> Result<Self, Error> {
        let used = used.into_iter().collect::<BTreeSet<_>>();
        (0..MAX_SERVICES)
            .map(Self)
            .find(|slot| !used.contains(slot))
            .ok_or_else(service_limit)
    }

    /// Refuses a new installation when every slot is taken.
    pub fn ensure_available(package_data: &Model<AllPackageData>) -> Result<(), Error> {
        Self::lowest_free(Self::held(package_data)?).map(|_| ())
    }

    pub fn allocate(package_data: &Model<AllPackageData>) -> Result<Self, Error> {
        Self::lowest_free(Self::held(package_data)?)
    }

    fn held(package_data: &Model<AllPackageData>) -> Result<Vec<Self>, Error> {
        package_data
            .as_entries()?
            .into_iter()
            .map(|(_, entry)| entry.as_container_slot().de())
            .collect()
    }

    pub fn index(self) -> u16 {
        self.0
    }

    pub fn ipv4(self) -> Ipv4Addr {
        Ipv4Addr::from(
            u32::from(CONTAINER_NET.network()) + FIRST_CONTAINER_HOST + u32::from(self.0),
        )
    }

    pub fn ipv6(self) -> Ipv6Addr {
        Ipv6Addr::from(
            u128::from(CONTAINER_NET_V6.network())
                + u128::from(FIRST_CONTAINER_HOST)
                + u128::from(self.0),
        )
    }

    /// Host uid and gid of the container's root, which also owns its user namespace.
    pub fn host_root(self) -> u32 {
        (u32::from(self.0) + 1) * ID_RANGE
    }

    /// Maps container ids onto the container's own host range.
    pub fn idmap(self) -> IdMap {
        IdMap {
            from_id: 0,
            to_id: self.host_root(),
            range: ID_RANGE,
        }
    }

    /// Host-side name of the container's veth.
    pub fn veth(self) -> String {
        format!("{VETH_PREFIX}{}", self.0)
    }
}

impl std::fmt::Display for ContainerSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Host uid of container root when every container shared one id range.
pub const LEGACY_HOST_ROOT: u32 = 100000;

fn rebase_legacy_id(id: u32, base: u32) -> Option<u32> {
    (LEGACY_HOST_ROOT..LEGACY_HOST_ROOT + ID_RANGE)
        .contains(&id)
        .then(|| id - LEGACY_HOST_ROOT + base)
}

/// Moves owners and named POSIX ACL identities into the same container ids at the base.
///
/// Stays on one filesystem and preserves set-id bits. A missing path is left alone.
pub async fn rebase_legacy_ids(path: std::path::PathBuf, base: u32) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || rebase_legacy_tree(&path, base))
        .await
        .with_kind(ErrorKind::Unknown)?
        .with_kind(ErrorKind::Filesystem)
}

fn rebase_legacy_tree(root: &std::path::Path, base: u32) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let device = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata.dev(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.dev() != device {
            continue;
        }
        let uid = rebase_legacy_id(metadata.uid(), base);
        let gid = rebase_legacy_id(metadata.gid(), base);
        if uid.is_some() || gid.is_some() {
            std::os::unix::fs::lchown(&path, uid, gid)?;
            let mode = metadata.mode();
            if !metadata.is_symlink() && mode & 0o6000 != 0 {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o7777))?;
            }
        }
        #[cfg(target_os = "linux")]
        if !metadata.is_symlink() {
            for attribute in ["system.posix_acl_access", "system.posix_acl_default"] {
                if let Some(mut acl) = xattr::get(&path, attribute)? {
                    if rebase_acl_ids(&mut acl, base)? {
                        xattr::set(&path, attribute, &acl)?;
                    }
                }
            }
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn rebase_acl_ids(acl: &mut [u8], base: u32) -> std::io::Result<bool> {
    // Linux POSIX ACL xattrs use version 2 and little-endian eight-byte entries.
    if acl.len() < 4 || acl[..4] != 2u32.to_le_bytes() || (acl.len() - 4) % 8 != 0 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut changed = false;
    for entry in acl[4..].chunks_exact_mut(8) {
        let tag = u16::from_le_bytes(entry[..2].try_into().unwrap());
        if matches!(tag, 0x02 | 0x08) {
            let id = u32::from_le_bytes(entry[4..].try_into().unwrap());
            if let Some(id) = rebase_legacy_id(id, base) {
                entry[4..].copy_from_slice(&id.to_le_bytes());
                changed = true;
            }
        }
    }
    Ok(changed)
}

fn service_limit() -> Error {
    Error::new(
        eyre!("{}", t!("lxc.slot.service-limit", max = MAX_SERVICES)),
        ErrorKind::InvalidRequest,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn addresses_start_after_the_two_server_addresses() {
        let first = ContainerSlot::new(0).unwrap();
        assert_eq!(first.ipv4(), Ipv4Addr::new(172, 16, 0, 3));
        assert_eq!(first.ipv6(), "fd00:3::3".parse::<Ipv6Addr>().unwrap());
        let last = ContainerSlot::new(MAX_SERVICES - 1).unwrap();
        assert_eq!(last.ipv4(), Ipv4Addr::new(172, 16, 4, 2));
        assert!(CONTAINER_NET.contains(&last.ipv4()));
        assert!(CONTAINER_NET_V6.contains(&last.ipv6()));
        for slot in [first, last] {
            assert_ne!(slot.ipv4(), HOST_IP);
            assert_ne!(slot.ipv4(), OS_LISTEN_IP);
            assert_ne!(slot.ipv6(), HOST_IPV6);
            assert_ne!(slot.ipv6(), OS_LISTEN_IPV6);
        }
    }

    #[test]
    fn os_listen_matches_both_families_and_mapped_ipv4_only() {
        for ip in [
            IpAddr::V4(OS_LISTEN_IP),
            IpAddr::V6(OS_LISTEN_IPV6),
            IpAddr::V6(OS_LISTEN_IP.to_ipv6_mapped()),
        ] {
            assert!(is_os_listen(ip), "{ip}");
        }
        let slot = ContainerSlot::new(0).unwrap();
        for ip in [
            IpAddr::V4(HOST_IP),
            IpAddr::V6(HOST_IPV6),
            IpAddr::V4(slot.ipv4()),
            IpAddr::V6(slot.ipv6()),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
        ] {
            assert!(!is_os_listen(ip), "{ip}");
        }
    }

    #[test]
    fn host_ranges_are_disjoint_and_above_the_host_ids() {
        let first = ContainerSlot::new(0).unwrap();
        assert_eq!(
            first.idmap(),
            IdMap {
                from_id: 0,
                to_id: 65536,
                range: 65536
            }
        );
        assert_eq!(first.host_root(), 65536);
        let last = ContainerSlot::new(MAX_SERVICES - 1).unwrap();
        assert_eq!(last.host_root(), 1024 << 16);
        let next = ContainerSlot::new(1).unwrap();
        assert_eq!(
            first.idmap().to_id + first.idmap().range,
            next.idmap().to_id
        );
        assert!(u64::from(last.host_root()) + u64::from(ID_RANGE) <= i32::MAX as u64);
    }

    #[test]
    fn allocation_takes_the_lowest_free_slot() {
        let used = [0, 1, 3].map(|i| ContainerSlot::new(i).unwrap());
        assert_eq!(ContainerSlot::lowest_free(used).unwrap().index(), 2);
        assert_eq!(ContainerSlot::lowest_free([]).unwrap().index(), 0);
    }

    #[test]
    fn allocation_refuses_the_1025th_service() {
        let all = (0..MAX_SERVICES).map(|i| ContainerSlot::new(i).unwrap());
        assert!(ContainerSlot::lowest_free(all).is_err());
        assert!(ContainerSlot::new(MAX_SERVICES).is_err());
    }

    #[tokio::test]
    async fn legacy_owners_move_to_the_new_base_and_others_stay() {
        use std::os::unix::fs::MetadataExt;
        if !nix::unistd::geteuid().is_root() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("dir");
        std::fs::create_dir(&dir).unwrap();
        let legacy = dir.join("legacy");
        let foreign = dir.join("foreign");
        std::fs::write(&legacy, b"").unwrap();
        std::fs::write(&foreign, b"").unwrap();
        if std::os::unix::fs::chown(&dir, Some(LEGACY_HOST_ROOT), Some(LEGACY_HOST_ROOT)).is_err() {
            return;
        }
        std::os::unix::fs::chown(
            &legacy,
            Some(LEGACY_HOST_ROOT + 1000),
            Some(LEGACY_HOST_ROOT + 999),
        )
        .unwrap();
        std::os::unix::fs::chown(&foreign, Some(42), Some(42)).unwrap();
        rebase_legacy_ids(root.path().to_owned(), 0).await.unwrap();
        let owner = |path: &std::path::Path| {
            let metadata = std::fs::symlink_metadata(path).unwrap();
            (metadata.uid(), metadata.gid())
        };
        assert_eq!(owner(&dir), (0, 0));
        assert_eq!(owner(&legacy), (1000, 999));
        assert_eq!(owner(&foreign), (42, 42));
        rebase_legacy_ids(root.path().to_owned(), 0).await.unwrap();
        assert_eq!(owner(&legacy), (1000, 999));
    }

    #[cfg(target_os = "linux")]
    const LEGACY_ACL: [u8; 60] = [
        2, 0, 0, 0, 1, 0, 7, 0, 255, 255, 255, 255, 2, 0, 4, 0, 42, 0, 0, 0, 2, 0, 4, 0, 136, 138,
        1, 0, 4, 0, 0, 0, 255, 255, 255, 255, 8, 0, 6, 0, 135, 138, 1, 0, 16, 0, 6, 0, 255, 255,
        255, 255, 32, 0, 0, 0, 255, 255, 255, 255,
    ];

    #[cfg(target_os = "linux")]
    fn rebased_acl() -> [u8; 60] {
        let mut expected = LEGACY_ACL;
        expected[24..28].copy_from_slice(&1000u32.to_le_bytes());
        expected[40..44].copy_from_slice(&999u32.to_le_bytes());
        expected
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn named_acl_entries_in_the_legacy_range_move_to_the_base() {
        let mut acl = LEGACY_ACL;
        assert!(rebase_acl_ids(&mut acl, 0).unwrap());
        assert_eq!(acl, rebased_acl());
        assert!(!rebase_acl_ids(&mut acl, 0).unwrap());
        assert!(rebase_acl_ids(&mut [1, 0, 0, 0], 0).is_err());
        assert!(rebase_acl_ids(&mut LEGACY_ACL[..59].to_vec(), 0).is_err());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn legacy_access_and_default_acls_keep_their_container_identities() {
        let root = tempfile::tempdir().unwrap();
        for attribute in ["system.posix_acl_access", "system.posix_acl_default"] {
            if let Err(error) = xattr::set(root.path(), attribute, &LEGACY_ACL) {
                eprintln!("skipped: legacy ids are unmapped or ACLs unsupported here ({error})");
                return;
            }
        }
        rebase_legacy_ids(root.path().to_owned(), 0).await.unwrap();
        for attribute in ["system.posix_acl_access", "system.posix_acl_default"] {
            let rebased = xattr::get(root.path(), attribute).unwrap().unwrap();
            assert_eq!(rebased, rebased_acl());
        }
    }

    #[test]
    fn veth_names_fit_the_interface_name_limit() {
        let last = ContainerSlot::new(MAX_SERVICES - 1).unwrap();
        assert!(last.veth().len() < libc::IFNAMSIZ);
    }
}
