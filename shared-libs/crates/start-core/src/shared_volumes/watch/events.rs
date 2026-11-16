use std::io::{self, IoSliceMut};
use std::os::fd::{AsRawFd, OwnedFd};
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use nix::sys::socket::{
    AddressFamily, ControlMessageOwned, MsgFlags, NetlinkAddr, SockFlag, SockProtocol, SockType,
    bind, recvmsg, setsockopt, socket, sockopt,
};
use tokio::io::unix::AsyncFd;
use tokio::sync::mpsc::Sender;

use super::Event;
use crate::prelude::*;
use crate::util::future::Until;
#[derive(Clone, Copy)]
enum Kind {
    Udev,
    Route,
}

impl Kind {
    fn event(self) -> Event {
        match self {
            Self::Udev => Event::Disks,
            Self::Route => Event::Network,
        }
    }
}

pub(super) async fn watch(send: Sender<Event>) {
    let listeners = futures::stream::FuturesUnordered::new();
    for kind in [Kind::Udev, Kind::Route] {
        let send = send.clone();
        listeners.push(
            async move {
                loop {
                    if let Err(error) = listen(kind, &send).await {
                        tracing::warn!(
                            "{}",
                            t!("shared-volumes.availability-watch-failed", error = error)
                        );
                    }
                    if send.is_closed() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
            .boxed(),
        );
    }
    listeners.push(
        async move {
            loop {
                if let Err(error) = browse(&send).await {
                    tracing::warn!(
                        "{}",
                        t!("shared-volumes.availability-watch-failed", error = error)
                    );
                }
                if send.is_closed() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
        .boxed(),
    );
    listeners.collect::<Vec<_>>().await;
}

fn open(kind: Kind) -> io::Result<AsyncFd<OwnedFd>> {
    let (protocol, groups) = match kind {
        Kind::Udev => (SockProtocol::NetlinkKObjectUEvent, 2),
        Kind::Route => (
            SockProtocol::NetlinkRoute,
            (libc::RTMGRP_LINK
                | libc::RTMGRP_IPV4_IFADDR
                | libc::RTMGRP_IPV6_IFADDR
                | libc::RTMGRP_IPV4_ROUTE
                | libc::RTMGRP_IPV6_ROUTE) as u32,
        ),
    };
    let socket = socket(
        AddressFamily::Netlink,
        SockType::Datagram,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        protocol,
    )?;
    setsockopt(&socket, sockopt::PassCred, &true)?;
    bind(socket.as_raw_fd(), &NetlinkAddr::new(0, groups))?;
    // OwnedFd keeps the registered descriptor open and stable until drop.
    unsafe { AsyncFd::register(socket) }.map_err(io::Error::from)
}

async fn listen(kind: Kind, send: &Sender<Event>) -> Result<(), Error> {
    let socket = open(kind)?;
    send.send(kind.event())
        .await
        .with_kind(ErrorKind::Network)?;
    let mut buffer = vec![0u8; 65536];
    loop {
        let result = socket.readable().await?.try_io(|socket| {
            let mut iov = [IoSliceMut::new(&mut buffer)];
            let mut control = nix::cmsg_space!(nix::sys::socket::UnixCredentials);
            let message = recvmsg::<NetlinkAddr>(socket.get_ref().as_raw_fd(), &mut iov, Some(&mut control), MsgFlags::empty())?;
            let trusted = match kind {
                Kind::Udev => message.cmsgs()?.any(|control| matches!(control, ControlMessageOwned::ScmCredentials(credentials) if credentials.uid() == 0)),
                Kind::Route => message.address.is_some_and(|address| address.pid() == 0),
            };
            Ok((message.bytes, trusted, message.flags.contains(MsgFlags::MSG_TRUNC)))
        });
        let (length, trusted, truncated) = match result {
            Ok(Ok(message)) => message,
            Ok(Err(error)) if error.raw_os_error() == Some(libc::ENOBUFS) => {
                send.send(kind.event())
                    .await
                    .with_kind(ErrorKind::Network)?;
                continue;
            }
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => continue,
        };
        if !trusted {
            continue;
        }
        if matches!(kind, Kind::Route) || truncated || block_event(&buffer[..length]) {
            send.send(kind.event())
                .await
                .with_kind(ErrorKind::Network)?;
        }
    }
}

fn block_event(message: &[u8]) -> bool {
    if message.len() < 40
        || &message[..8] != b"libudev\0"
        || message[8..12] != [0xfe, 0xed, 0xca, 0xfe]
    {
        return false;
    }
    let offset = u32::from_ne_bytes(message[16..20].try_into().unwrap()) as usize;
    let length = u32::from_ne_bytes(message[20..24].try_into().unwrap()) as usize;
    if offset < 40 {
        return false;
    }
    let Some(end) = offset.checked_add(length) else {
        return false;
    };
    let Some(properties) = message.get(offset..end) else {
        return false;
    };
    let mut fields = properties.split(|byte| *byte == 0);
    fields.clone().any(|field| field == b"SUBSYSTEM=block")
        && !fields
            .clone()
            .any(|field| field.starts_with(b"DEVNAME=/dev/loop"))
        && fields.any(|field| {
            matches!(
                field,
                b"ACTION=add" | b"ACTION=remove" | b"ACTION=change" | b"ACTION=move"
            )
        })
}

async fn browse(send: &Sender<Event>) -> Result<(), Error> {
    let connection = zbus::Connection::system().await?;
    let server = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Avahi",
        "/",
        "org.freedesktop.Avahi.Server2",
    )
    .await?;
    let owners = server.receive_owner_changed().await?;
    let path: zbus::zvariant::OwnedObjectPath = server
        .call(
            "ServiceBrowserPrepare",
            &(-1i32, -1i32, "_smb._tcp", "", 0u32),
        )
        .await?;
    let browser = zbus::Proxy::new(
        &connection,
        server.destination().clone(),
        path,
        "org.freedesktop.Avahi.ServiceBrowser",
    )
    .await?;
    let mut signals = browser.receive_all_signals().await?;
    browser.call::<_, _, ()>("Start", &()).await?;
    let mut until = Until::new().with_stream(owners.map(|_| Ok(())));
    until
        .run(async {
            loop {
                let signal = signals.next().await.ok_or_else(|| {
                    Error::new(eyre!("Avahi service browser closed"), ErrorKind::Network)
                })?;
                match signal.header().member().map(|member| member.as_str()) {
                    Some("ItemNew" | "ItemRemove") => {
                        send.send(Event::Network)
                            .await
                            .with_kind(ErrorKind::Network)?;
                    }
                    Some("Failure") => {
                        return Err(Error::new(
                            eyre!("Avahi service browser failed"),
                            ErrorKind::Network,
                        ));
                    }
                    _ => {}
                }
            }
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(properties: &[u8]) -> Vec<u8> {
        let mut message = vec![0; 40];
        message[..8].copy_from_slice(b"libudev\0");
        message[8..12].copy_from_slice(&[0xfe, 0xed, 0xca, 0xfe]);
        message[16..20].copy_from_slice(&40u32.to_ne_bytes());
        message[20..24].copy_from_slice(&(properties.len() as u32).to_ne_bytes());
        message.extend(properties);
        message
    }

    #[test]
    fn accepts_udev_block_updates_not_early_kernel_events() {
        for action in ["add", "remove", "change", "move"] {
            assert!(block_event(&event(
                format!("ACTION={action}\0SUBSYSTEM=block\0ID_FS_UUID=abcd\0").as_bytes()
            )));
        }
        assert!(!block_event(
            b"add@/devices/block/sda\0SUBSYSTEM=block\0ACTION=add\0"
        ));
        assert!(!block_event(&event(b"ACTION=add\0SUBSYSTEM=net\0")));
        assert!(!block_event(&event(b"ACTION=bind\0SUBSYSTEM=block\0")));
    }

    #[test]
    fn service_loop_lifetimes_do_not_invalidate_physical_drive_discovery() {
        for action in ["add", "remove", "change", "move"] {
            for node in ["/dev/loop0", "/dev/loop123"] {
                assert!(!block_event(&event(
                    format!("ACTION={action}\0SUBSYSTEM=block\0DEVNAME={node}\0").as_bytes()
                )));
            }
            assert!(block_event(&event(
                format!("ACTION={action}\0SUBSYSTEM=block\0DEVNAME=/dev/sda1\0").as_bytes()
            )));
        }
    }

    #[test]
    fn rejects_truncated_or_invalid_udev_headers() {
        let mut message = event(b"SUBSYSTEM=block\0ACTION=add\0");
        for length in 0..message.len() {
            assert!(!block_event(&message[..length]));
        }
        message[16..20].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert!(!block_event(&message));
        message[16..20].copy_from_slice(&0u32.to_ne_bytes());
        assert!(!block_event(&message));
    }
}
