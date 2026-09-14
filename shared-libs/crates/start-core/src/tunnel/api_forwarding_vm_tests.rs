use std::net::SocketAddrV6;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::time::Duration;

use futures::FutureExt;
use tokio::process::Command;

use super::*;
use crate::tunnel::context::TunnelConfig;
use crate::tunnel::forward::lease::{self, LeaseKey};
use crate::util::Invoke;

async fn rules(family: &str) -> String {
    String::from_utf8(
        Command::new("/usr/sbin/nft")
            .args(["-a", "list", "table", family, "startos"])
            .invoke(ErrorKind::Network)
            .await
            .unwrap(),
    )
    .unwrap()
}

async fn exercise(ctx: &TunnelContext, controls: &Path) {
    let target: SocketAddrV4 = "192.0.2.2:42000".parse().unwrap();
    let source: SocketAddrV4 = "198.51.100.2:41000".parse().unwrap();
    let prefix: Ipv6Net = "2001:db8:1234::/64".parse().unwrap();
    let gua = wg6::host_v6(prefix, *target.ip());
    ctx.db
        .mutate(|db| {
            db.as_wg_mut().as_subnets_mut().mutate(|subnets| {
                let mut subnet = WgSubnetConfig::new("forwarding-vm-test".into());
                subnet.ipv6 = Some(prefix);
                subnet.wan_ip = Some(*source.ip());
                subnet.clients.0.insert(
                    *target.ip(),
                    WgConfig::generate("fixture".into(), WgClientKind::Server),
                );
                subnets.0.insert("192.0.2.0/24".parse().unwrap(), subnet);
                Ok(())
            })
        })
        .await
        .result
        .unwrap();

    add_forward(
        ctx.clone(),
        AddPortForwardParams {
            external_port: source.port(),
            target,
            label: None,
            sni: vec![],
            count: Some(3),
        },
    )
    .await
    .unwrap();
    for enabled in [false, false, true, true] {
        set_forward_enabled(
            ctx.clone(),
            SetPortForwardEnabledParams {
                source,
                enabled,
                hostname: None,
            },
        )
        .await
        .unwrap();
        let db = ctx.db.peek().await;
        assert!(matches!(
            db.as_port_forwards().de().unwrap().0[&source],
            PortForward::Dnat { count: 3, enabled: actual, .. } if actual == enabled
        ));
        let installed = rules("ip").await;
        if enabled {
            for offset in 0..3 {
                assert!(
                    installed.contains(&format!(
                        "{} : {} . {}",
                        source.port() + offset,
                        target.ip(),
                        target.port() + offset
                    )),
                    "missing DNAT range offset {offset}: {installed}"
                );
            }
        } else {
            assert!(!installed.contains("198.51.100.2"), "{installed}");
        }
    }
    remove_forward(
        ctx.clone(),
        RemovePortForwardParams {
            source,
            hostname: None,
        },
    )
    .await
    .unwrap();
    assert!(!rules("ip").await.contains("198.51.100.2"));
    eprintln!("PASS: actual set-enabled handler preserves all three DNAT offsets");

    add_pinhole(
        ctx.clone(),
        AddPinholeParams {
            gua,
            external_port: 43000,
            internal_port: Some(44000),
            label: None,
            count: Some(3),
        },
    )
    .await
    .unwrap();
    let key = SocketAddrV6::new(gua, 43000, 0, 0);
    let lease_key = LeaseKey::Pinhole(key);
    lease::stamp(ctx, lease_key.clone(), 3600);
    let expiry = ctx.leases.peek(|leases| leases[&lease_key]);
    let before = rules("ip6").await;
    let tag = format!("pinhole:{key}");
    assert_eq!(before.lines().filter(|line| line.contains(&tag)).count(), 2);
    std::fs::write(controls.join("fail-delete"), "").unwrap();
    let failed = remove_pinhole(
        ctx.clone(),
        RemovePinholeParams {
            gua,
            external_port: 43000,
        },
    )
    .await;
    std::fs::remove_file(controls.join("fail-delete")).unwrap();
    assert!(failed.is_err(), "failed nft deletion reported success");
    assert!(
        ctx.db
            .peek()
            .await
            .as_pinholes6()
            .de()
            .unwrap()
            .0
            .contains_key(&key)
    );
    assert_eq!(
        ctx.leases.peek(|leases| leases.get(&lease_key).copied()),
        Some(expiry)
    );
    assert_eq!(rules("ip6").await, before);
    remove_pinhole(
        ctx.clone(),
        RemovePinholeParams {
            gua,
            external_port: 43000,
        },
    )
    .await
    .unwrap();
    assert!(
        !ctx.db
            .peek()
            .await
            .as_pinholes6()
            .de()
            .unwrap()
            .0
            .contains_key(&key)
    );
    assert!(!ctx.leases.peek(|leases| leases.contains_key(&lease_key)));
    assert!(!rules("ip6").await.contains(&tag));
    eprintln!(
        "PASS: failed pinhole removal retains DB, exact rules and lease; same-key retry removes all"
    );

    std::fs::write(controls.join("barrier"), "198.51.100.2").unwrap();
    let add = add_forward(
        ctx.clone(),
        AddPortForwardParams {
            external_port: source.port(),
            target,
            label: None,
            sni: vec![],
            count: Some(3),
        },
    );
    tokio::pin!(add);
    let entered = async {
        while !controls.join("entered").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::select! {
        result = &mut add => panic!("add finished before nft barrier: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(10), entered) => result.unwrap(),
    }
    let remove = remove_forward(
        ctx.clone(),
        RemovePortForwardParams {
            source,
            hostname: None,
        },
    );
    tokio::pin!(remove);
    let early = tokio::time::timeout(Duration::from_millis(150), &mut remove).await;
    std::fs::write(controls.join("release"), "").unwrap();
    let add_result = add.await;
    let (serialized, remove_result) = match early {
        Ok(result) => (false, result),
        Err(_) => (true, remove.await),
    };
    add_result.unwrap();
    remove_result.unwrap();
    let db_present = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .unwrap()
        .0
        .contains_key(&source);
    let owner_present = ctx
        .active_forwards
        .peek(|active| active.contains_key(&source));
    let installed = rules("ip").await;
    let rule_present = installed.contains("198.51.100.2");
    eprintln!(
        "race outcome: serialized={serialized}, db={db_present}, owner={owner_present}, rule={rule_present}"
    );
    std::fs::write(controls.join("race-rules.txt"), &installed).unwrap();
    assert!(
        serialized,
        "remove overtook the admitted add's nft transaction"
    );
    assert!(!db_present);
    assert!(!owner_present);
    assert!(!rule_present);
    eprintln!("PASS: concurrent remove waits for add and leaves no ownerless nft rule");
}

#[tokio::test]
#[ignore = "requires disposable StartOS VM, real nft, and process-scoped failure/barrier wrapper"]
async fn forwarding_handlers_vm() {
    let controls = std::env::var_os("STARTOS_FORWARDING_VM_TEST")
        .expect("run only in a disposable VM with the forwarding harness");
    let controls = Path::new(&controls);
    assert!(Path::new("/run/startos-forwarding-vm-test").exists());
    assert!(
        !controls.join("db").exists(),
        "use a fresh harness run directory"
    );
    let ctx = TunnelContext::init(&TunnelConfig {
        datadir: Some(controls.join("db")),
        ..Default::default()
    })
    .await
    .unwrap();
    let result = AssertUnwindSafe(exercise(&ctx, controls))
        .catch_unwind()
        .await;
    let _ = std::fs::remove_file(controls.join("fail-delete"));
    std::fs::write(controls.join("release"), "").unwrap();
    let drained = ctx.shutdown_forwarding().await;
    assert!(drained.is_ok(), "forwarding shutdown failed: {drained:?}");
    assert!(!rules("ip").await.contains("198.51.100.2"));
    assert!(!rules("ip6").await.contains("pinhole:"));
    eprintln!("PASS: shutdown_forwarding completed and fixture nft rules are absent");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
