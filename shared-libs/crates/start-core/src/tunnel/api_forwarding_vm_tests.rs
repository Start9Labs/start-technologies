use std::net::SocketAddrV6;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::time::Duration;

use futures::FutureExt;
use tokio::process::Command;

use super::*;
use crate::net::port_map::server::GatewayBackend;
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

async fn exercise_sni_reaper(ctx: &TunnelContext) {
    let source: SocketAddrV4 = "198.51.100.2:47000".parse().unwrap();
    let target: SocketAddrV4 = "192.0.2.2:48000".parse().unwrap();
    let hostnames = vec!["reaper.example.com".to_string()];
    GatewayBackend::add_sni_forward(ctx, source, target, &hostnames, Some(3600))
        .await
        .unwrap();
    ctx.persist_fallback_forward(source, target, Some(3600), true, None)
        .await
        .unwrap();
    for fallback in [false, true] {
        let key = if fallback {
            LeaseKey::SniFallback(source)
        } else {
            LeaseKey::Sni {
                source,
                hostname: hostnames[0].clone(),
            }
        };
        let guard = ctx.forward_write_lock.lock().await;
        ctx.leases.mutate(|leases| {
            leases.insert(
                key.clone(),
                std::time::Instant::now() - Duration::from_secs(1),
            );
        });
        let renew = async {
            if fallback {
                ctx.persist_fallback_forward(source, target, Some(3600), true, None)
                    .await
            } else {
                GatewayBackend::add_sni_forward(ctx, source, target, &hostnames, Some(3600)).await
            }
        };
        let reap = async {
            if fallback {
                lease::reap_sni_fallback(ctx, source).await
            } else {
                lease::reap_sni(ctx, source, &hostnames[0]).await
            }
        };
        tokio::pin!(renew, reap);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut renew)
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut reap)
                .await
                .is_err()
        );
        drop(guard);
        let (renewed, reaped) = tokio::join!(renew, reap);
        renewed.unwrap();
        reaped.unwrap();
        assert!(
            ctx.leases
                .peek(|leases| leases[&key] > std::time::Instant::now())
        );
        let forward = ctx.db.peek().await.as_port_forwards().de().unwrap().0[&source].clone();
        assert!(
            matches!(forward, PortForward::Sni { routes, fallback } if routes.contains_key(&hostnames[0]) && fallback.is_some())
        );

        let guard = ctx.forward_write_lock.lock().await;
        ctx.leases.mutate(|leases| {
            leases.insert(
                key.clone(),
                std::time::Instant::now() - Duration::from_secs(1),
            );
        });
        let reap = async {
            if fallback {
                lease::reap_sni_fallback(ctx, source).await
            } else {
                lease::reap_sni(ctx, source, &hostnames[0]).await
            }
        };
        tokio::pin!(reap);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut reap)
                .await
                .is_err()
        );
        ctx.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|forwards| {
                    let PortForward::Sni {
                        routes,
                        fallback: route_fallback,
                    } = forwards.0.get_mut(&source).unwrap()
                    else {
                        unreachable!()
                    };
                    if fallback {
                        route_fallback.as_mut().unwrap().auto = false;
                    } else {
                        routes.get_mut(&hostnames[0]).unwrap().auto = false;
                    }
                    Ok(())
                })
            })
            .await
            .result
            .unwrap();
        drop(guard);
        reap.await.unwrap();
        let forward = ctx.db.peek().await.as_port_forwards().de().unwrap().0[&source].clone();
        assert!(
            matches!(forward, PortForward::Sni { routes, fallback } if routes.contains_key(&hostnames[0]) && fallback.is_some())
        );
    }
    GatewayBackend::remove_sni_forward(ctx, source, target, &hostnames)
        .await
        .unwrap();
    ctx.remove_sni_fallback(source, target).await.unwrap();
    eprintln!(
        "PASS: queued SNI reapers preserve renewed leases and manually-owned routes/fallbacks"
    );
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
    let before = rules("ip").await;
    std::fs::write(controls.join("fail-delete"), "").unwrap();
    for _ in 0..2 {
        assert!(
            set_forward_enabled(
                ctx.clone(),
                SetPortForwardEnabledParams {
                    source,
                    enabled: false,
                    hostname: None
                },
            )
            .await
            .is_err()
        );
        assert_eq!(rules("ip").await, before);
    }
    std::fs::remove_file(controls.join("fail-delete")).unwrap();
    set_forward_enabled(
        ctx.clone(),
        SetPortForwardEnabledParams {
            source,
            enabled: false,
            hostname: None,
        },
    )
    .await
    .unwrap();
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
    eprintln!("PASS: two failed disables report failure; retry withdraws exact DNAT rules");

    GatewayBackend::add_forward(ctx, source, target, 3, *target.ip(), Some(3600))
        .await
        .unwrap();
    assert!(
        !GatewayBackend::remove_forward_by_source(ctx, source, "192.0.2.3".parse().unwrap())
            .await
            .unwrap()
    );
    let lease_key = LeaseKey::Dnat(source);
    let expiry = ctx.leases.peek(|leases| leases[&lease_key]);
    let before = rules("ip").await;
    let entry = ctx.db.peek().await.as_port_forwards().de().unwrap().0[&source].clone();
    std::fs::write(controls.join("fail-delete"), "").unwrap();
    assert!(
        GatewayBackend::remove_forward(ctx, *target.ip(), target.port())
            .await
            .is_err()
    );
    assert!(
        GatewayBackend::remove_forward_by_source(ctx, source, *target.ip())
            .await
            .is_err()
    );
    assert_eq!(rules("ip").await, before);
    assert_eq!(
        serde_json::to_value(
            ctx.db.peek().await.as_port_forwards().de().unwrap().0[&source].clone()
        )
        .unwrap(),
        serde_json::to_value(entry).unwrap()
    );
    assert_eq!(
        ctx.leases.peek(|leases| leases.get(&lease_key).copied()),
        Some(expiry)
    );
    std::fs::remove_file(controls.join("fail-delete")).unwrap();
    assert!(
        GatewayBackend::remove_forward_by_source(ctx, source, *target.ip())
            .await
            .unwrap()
    );
    GatewayBackend::remove_forward(ctx, *target.ip(), target.port())
        .await
        .unwrap();
    assert!(
        !GatewayBackend::remove_forward_by_source(ctx, source, *target.ip())
            .await
            .unwrap()
    );
    assert!(!ctx.leases.peek(|leases| leases.contains_key(&lease_key)));
    assert!(
        !ctx.db
            .peek()
            .await
            .as_port_forwards()
            .de()
            .unwrap()
            .0
            .contains_key(&source)
    );
    assert!(!rules("ip").await.contains("198.51.100.2"));
    eprintln!("PASS: PCP and IGD deletion failures retain exact DB, lease and rules for retry");
    eprintln!("PASS: actual set-enabled handler preserves all three DNAT offsets");

    let failed_key = SocketAddrV6::new(gua, 45000, 0, 0);
    let failed_lease = LeaseKey::Pinhole(failed_key);
    std::fs::write(controls.join("fail-add"), "").unwrap();
    assert!(
        GatewayBackend::add_pinhole(ctx, gua, 45000, 46000, 1, Some(3600))
            .await
            .is_err()
    );
    std::fs::remove_file(controls.join("fail-add")).unwrap();
    assert!(ctx.db.peek().await.as_pinholes6().de().unwrap().0[&failed_key].auto);
    assert!(ctx.leases.peek(|leases| leases.contains_key(&failed_lease)));
    assert!(
        !rules("ip6")
            .await
            .contains(&format!("pinhole:{failed_key}"))
    );
    GatewayBackend::remove_pinhole(ctx, gua, 45000)
        .await
        .unwrap();
    assert!(!ctx.leases.peek(|leases| leases.contains_key(&failed_lease)));
    eprintln!("PASS: first auto pinhole apply failure retains expiring intent");

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

    exercise_sni_reaper(ctx).await;

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
    let _ = std::fs::remove_file(controls.join("fail-add"));
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
