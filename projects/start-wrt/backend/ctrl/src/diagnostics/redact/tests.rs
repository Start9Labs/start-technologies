use super::*;

const NETWORK: &str = "config interface 'wan'
\toption proto 'pppoe'
\toption device 'eth1.7'
\toption username '1und1/0012345678-001@online.de'
\toption password 'Pa55/word\"x'

config interface 'wg_work'
\toption proto 'wireguard'
\toption private_key 'BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc='
\toption endpoint_host 'vpn.example.net'
";

const STARTWRT: &str = "config profile 'voorv'
\toption fullname 'Kids'
\toption interface 'voorv'
\toption vlan_tag '20'
";

const DDNS: &str = "config service 'wan'
\toption lookup_host 'Home.Example.org'
\toption domain 'home.example.org'
\toption username 'alice'
\toption password 'ddns-token-123'
";

fn redactor(configs: &[(&str, &str)], macs: &[&str]) -> Redactor {
    let mut known = Known::default();
    for (name, text) in configs {
        known.add_config(name, text);
    }
    known.add_macs(macs.iter().copied());
    Redactor::new(known, [7; 32])
}

fn plain() -> Redactor {
    redactor(&[], &[])
}

fn configured() -> Redactor {
    redactor(
        &[("network", NETWORK), ("startwrt", STARTWRT), ("ddns", DDNS)],
        &["D0:40:EF:3E:02:43"],
    )
}

/// The token, `<kind-hex>`, that follows `prefix` in `line`.
fn token_after<'a>(line: &'a str, prefix: &str) -> &'a str {
    let start = line.find(prefix).expect(prefix) + prefix.len();
    let end = start + line[start..].find('>').expect("token end") + 1;
    &line[start..end]
}

#[test]
fn pppoe_credentials_in_a_failed_notify_are_removed() {
    let line = r#"Mon Nov  9 10:00:00 2026 daemon.notice netifd: wan (1234): Command failed: ubus call network.interface notify_proto { "action": 1, "command": [ "\/usr\/sbin\/pppd", "nodetach", "ifname", "pppoe-wan", "user", "1und1\/0012345678-001@online.de", "password", "Pa55\/word\"x", "ip-up-script", "\/lib\/netifd\/ppp-up" ], "interface": "wan" } (Permission denied)"#;
    for redactor in [plain(), configured()] {
        let out = redactor.line(line, false);
        assert!(!out.contains("Pa55"), "{out}");
        assert!(!out.contains("0012345678"), "{out}");
        assert!(out.contains(r#""password", "[redacted]""#), "{out}");
        assert!(out.contains("@online.de"), "{out}");
        assert!(out.ends_with("(Permission denied)"), "{out}");
    }
}

#[test]
fn configured_secrets_are_removed_in_every_encoding() {
    let redactor = configured();
    for line in [
        "raw ddns-token-123 here",
        r#"json "Pa55\/word\"x" here"#,
        r#"json "Pa55/word\"x" here"#,
        "url ?q=Pa55%2Fword%22x here",
        "wg BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc= here",
    ] {
        let out = redactor.line(line, false);
        assert!(out.contains("[redacted]"), "{out}");
        assert!(
            !out.contains("Pa55") && !out.contains("ddns-token") && !out.contains("BwcH"),
            "{out}"
        );
    }
}

#[test]
fn a_secret_on_a_line_uciedit_rejects_is_still_known() {
    let redactor = redactor(
        &[(
            "wireless",
            "config wifi-iface 'x'\n\toption key 'abc\"defghi\n",
        )],
        &[],
    );
    let out = redactor.line(r#"found abc"defghi in a log"#, false);
    assert_eq!(out, "found [redacted] in a log");
    let out = plain().line(
        r#"ERROR startwrt::wifi: parse error: option key 'abc"defghi"#,
        false,
    );
    assert!(!out.contains("defghi"), "{out}");
}

#[test]
fn daemons_echoing_a_rejected_key_are_redacted() {
    let redactor = plain();
    assert_eq!(
        redactor.line("hostapd: Line 3: Invalid PSK 'short'pw'.", false),
        "hostapd: Line 3: Invalid PSK '[redacted]'."
    );
    assert_eq!(
        redactor.line(
            "netifd: wg_x (9): Key is not the correct length or format: `nope'",
            false
        ),
        "netifd: wg_x (9): Key is not the correct length or format: `[redacted]'"
    );
}

#[test]
fn a_typed_login_name_is_removed_unless_it_is_root() {
    let redactor = plain();
    assert_eq!(
        redactor.line(
            "luci: failed login on / for MyPassw0rd from 127.0.0.1",
            false
        ),
        "luci: failed login on / for [redacted] from 127.0.0.1"
    );
    let accepted = "luci: accepted login on / for root from 127.0.0.1";
    assert_eq!(redactor.line(accepted, false), accepted);
    assert_eq!(
        redactor.line(
            "login[99]: invalid password for 'hunter22' on 'ttyS0'",
            false
        ),
        "login[99]: invalid password for '[redacted]' on 'ttyS0'"
    );
}

#[test]
fn login_names_with_spaces_are_removed_without_live_config() {
    let out = plain().line(
        "luci: failed login on / for old password with spaces from 192.168.0.2",
        false,
    );
    assert_eq!(
        out,
        "luci: failed login on / for [redacted] from 192.168.0.2"
    );
}

#[test]
fn credentials_in_urls_are_removed() {
    let redactor = plain();
    let out = redactor.line(
        "request for url (https://alice:pw@registry.example.org/rpc/v0)",
        false,
    );
    assert!(
        out.contains("https://[redacted]@") && !out.contains("alice"),
        "{out}"
    );
    assert_eq!(
        redactor.line("for url (http://127.0.0.1:8080/cgi-bin/luci/;stok=0a1b2c/admin?token=xyz&page=2)", false),
        "for url (http://127.0.0.1:8080/cgi-bin/luci/;stok=[redacted]/admin?token=[redacted]&page=2)"
    );
}

#[test]
fn query_secrets_include_slashes_and_semicolons() {
    for secret in ["old/token/secret", "old;token;secret"] {
        let out = plain().line(
            &format!("request https://example.org/?token={secret}&page=2"),
            false,
        );
        assert_eq!(out, "request https://example.org/?token=[redacted]&page=2");
    }
}

#[test]
fn a_query_secret_after_a_semicolon_keeps_its_slashes() {
    assert_eq!(
        plain().line(
            "request https://example.org/x;stok=abc/y?page=2;token=old/token?secret#top",
            false
        ),
        "request https://example.org/x;stok=[redacted]/y?page=2;token=[redacted]#top"
    );
}

#[test]
fn a_mac_keeps_its_vendor_and_one_token_across_daemons() {
    let redactor = plain();
    let dhcp = redactor.line(
        "dnsmasq-dhcp[1]: DHCPACK(br-lan) 192.168.0.5 d0:40:ef:3e:02:43 Johns-iPhone",
        false,
    );
    let wifi = redactor.line(
        "hostapd: phy0-ap0: AP-STA-CONNECTED D0:40:EF:3E:02:43 auth_alg=open",
        false,
    );
    let tag = token_after(&dhcp, "d0:40:ef:");
    assert!(tag.starts_with("<mac-"), "{dhcp}");
    assert_eq!(token_after(&wifi, "D0:40:EF:"), tag, "{wifi}");
    assert!(
        !dhcp.contains("3e:02:43") && !dhcp.contains("Johns"),
        "{dhcp}"
    );
    assert!(
        dhcp.contains("192.168.0.5") && dhcp.contains("<name-"),
        "{dhcp}"
    );

    let random = redactor.line("AP-STA-CONNECTED ee:cd:6e:77:b0:37", false);
    assert!(
        random.contains("<rmac-") && !random.contains("ee:cd"),
        "{random}"
    );
}

#[test]
fn group_and_unset_macs_are_kept() {
    let line = "33:33:00:00:00:01 ff:ff:ff:ff:ff:ff 00:00:00:00:00:00";
    assert_eq!(plain().line(line, false), line);
}

#[test]
fn only_public_ipv4_addresses_are_replaced() {
    let redactor = plain();
    let out = redactor.line(
        "udhcpc: lease of 84.12.34.56 obtained from 84.12.34.1, server 84.12.34.56:443",
        false,
    );
    assert!(!out.contains("84.12"), "{out}");
    let first = token_after(&out, "lease of ");
    assert_eq!(token_after(&out, "server "), first, "{out}");

    let kept =
        "192.168.178.117 10.0.0.1 100.64.3.4 169.254.1.1 1.1.1.1 203.0.113.9 v1.2.3.4 1.2.3.4.5";
    assert_eq!(redactor.line(kept, false), kept);
}

#[test]
fn ipv6_keeps_its_shape_with_the_prefix_and_mac_replaced() {
    let redactor = plain();
    let prefix = redactor.line(
        "odhcpd: rfc9096: br-lan: changed 2a02:810d:4b40:1200::/56",
        false,
    );
    let address = redactor.line(
        "avahi: record for 2a02:810d:4b40:1200:1a66:96ff:fea2:6282 on br-lan",
        false,
    );
    let gua = token_after(&prefix, "changed ");
    assert!(prefix.ends_with(&format!("{gua}:1200::/56")), "{prefix}");
    let eui = redactor.mac("18:66:96:a2:62:82").unwrap();
    let mac_tag = &eui["18:66:96:".len()..];
    assert!(
        address.contains(&format!("{gua}:1200:{mac_tag} on")),
        "{address}"
    );

    assert_eq!(
        redactor.line("via fe80::1a66:96ff:fea2:6282%br-lan", false),
        format!("via fe80::{mac_tag}%br-lan")
    );
    let ula = redactor.line(
        "prefix fd01:6a13:6ff7::/60 and fd01:6a13:6ff7:4650::1",
        false,
    );
    let token = token_after(&ula, "prefix ");
    assert!(token.starts_with("<ula-") && !ula.contains("6a13"), "{ula}");
    assert!(ula.ends_with(&format!("{token}:4650::1")), "{ula}");
}

#[test]
fn text_shaped_like_an_address_is_untouched() {
    for line in [
        "Mon Nov  9 19:07:26 2026 kern.info kernel: [    1.5] pci 0000:01:00.0: BAR 0",
        "WARN startwrt::system: Failed to check for updates",
        "loopback ::1 and fe80::1 and ff02::1",
        "addr: fe80::1: no route",
    ] {
        assert_eq!(plain().line(line, false), line);
    }
}

#[test]
fn key_fingerprints_and_wireguard_keys_become_tokens() {
    let redactor = plain();
    let out = redactor.line(
        "dropbear[5]: Pubkey auth succeeded for 'root' with ssh-ed25519 key SHA256:CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk from 192.168.0.2:5555",
        false,
    );
    assert!(
        out.contains("SHA256:<key-") && !out.contains("CQkJ"),
        "{out}"
    );
    let out = redactor.line(
        "peer BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc= handshake",
        false,
    );
    assert!(out.contains("<wgkey-") && !out.contains("BwcH"), "{out}");
}

#[test]
fn owned_domains_keep_only_their_top_level_domain() {
    let redactor = configured();
    for line in [
        "ddns-scripts[3]: wan: Get registered/public IP for 'Home.Example.org' failed",
        "WARN port-control: dropping SNI route nas.home.example.org on 84.1.2.3:443 -> x",
        "endpoint VPN.EXAMPLE.NET unreachable",
    ] {
        let out = redactor.line(line, false);
        assert!(!out.to_lowercase().contains("example"), "{out}");
        assert!(out.contains("<dom-"), "{out}");
    }
    let out = plain().line("dropping SNI route nas.other.com on x", false);
    assert!(out.contains("<dom-") && out.contains(".com on"), "{out}");
}

#[test]
fn configured_usernames_keep_their_realm() {
    let out = configured().line(
        "pppd[7]: Remote message: user 1und1/0012345678-001@online.de unknown",
        false,
    );
    assert!(
        out.contains("<user-") && out.contains("@online.de unknown"),
        "{out}"
    );
    assert!(!out.contains("0012345678"), "{out}");
}

#[test]
fn activity_names_become_profiles_or_tokens() {
    let redactor = configured();
    let out = redactor.line(
        "startwrt-ctrld: INFO activity: startwrt-activity [vpn-server.peer-added] OK Added peer 'Alice phone' to inbound VPN for profile 'Kids'",
        true,
    );
    assert!(out.contains("for profile '<profile-voorv>'"), "{out}");
    assert!(
        !out.contains("Alice") && out.contains("peer '<name-"),
        "{out}"
    );

    let out = redactor.line(
        "auth.login OK Logged in from 192.168.20.5 (Kids profile) using curl/8.1",
        true,
    );
    assert!(out.contains("(<profile-voorv> profile)"), "{out}");

    for kept in [
        "system.remote-access OK Updated remote access to 'lan'",
        "system.timezone OK Timezone 'Europe/Berlin' not recognized; kept UTC",
    ] {
        assert_eq!(redactor.line(kept, true), kept);
    }

    let out = redactor.line(
        "Outbound VPN 'Work' now connects over the Internet — a VPN connecting through another needs its server's IP address",
        true,
    );
    assert!(
        !out.contains("'Work'") && out.contains("server's IP"),
        "{out}"
    );

    let syslog = "ddns-scripts[3]: wan: retry for 'Kids'";
    assert_eq!(redactor.line(syslog, false), syslog);
}

#[test]
fn vpn_source_profiles_are_replaced() {
    let out = configured().line(
        "auth.login OK Logged in from 10.0.0.2 (Kids profile, VPN) using curl/8.1",
        true,
    );
    assert_eq!(
        out,
        "auth.login OK Logged in from 10.0.0.2 (<profile-voorv> profile, VPN) using curl/8.1"
    );
}

#[test]
fn parenthesized_profile_names_are_replaced() {
    let redactor = redactor(
        &[(
            "startwrt",
            "config profile 'work'\n\toption fullname 'Alice (work)'\n\toption interface 'work'\n",
        )],
        &[],
    );
    assert_eq!(
        redactor.line("Logged in from 10.0.0.2 (Alice (work) profile, VPN)", true),
        "Logged in from 10.0.0.2 (<profile-work> profile, VPN)"
    );
}

#[test]
fn token_like_names_are_private() {
    let redactor = configured();
    assert_eq!(
        redactor.line("Added peer '<Alice phone>'", true),
        format!("Added peer '{}'", redactor.name("<Alice phone>"))
    );
    assert_eq!(
        redactor.line("Changed domain 'home.example.org'", true),
        format!("Changed domain '{}'", redactor.domain("home.example.org"))
    );
}

#[test]
fn possessive_names_are_replaced_as_whole_values() {
    let redactor = redactor(
        &[(
            "startwrt",
            "config profile 'kids'\n\toption fullname \"Kid's devices\"\n\toption interface 'kids'\n",
        )],
        &[],
    );
    let out = redactor.line(
        "Added peer 'Alice's personal phone' to inbound VPN for profile 'Kid's devices'",
        true,
    );
    assert_eq!(
        out,
        format!(
            "Added peer '{}' to inbound VPN for profile '<profile-kids>'",
            redactor.name("Alice's personal phone")
        )
    );
}

#[test]
fn names_with_an_apostrophe_beside_a_space_are_replaced_whole() {
    let redactor = configured();
    for name in [
        "James' iPhone",
        "Rock 'n roll",
        "Alice!",
        "Bob 'Work' laptop",
    ] {
        let out = redactor.line(
            &format!("Added peer '{name}' to inbound VPN for profile 'Kids'"),
            true,
        );
        assert_eq!(
            out,
            format!(
                "Added peer '{}' to inbound VPN for profile '<profile-voorv>'",
                redactor.name(name)
            )
        );
    }
}

#[test]
fn known_macs_written_as_bare_hex_are_replaced() {
    let redactor = configured();
    let tag = redactor.mac("d0:40:ef:3e:02:43").unwrap();
    let tag = &tag["d0:40:ef:".len()..];
    assert_eq!(
        redactor.line("uci: section host_d040ef3e0243 and GW-D040EF3E0243", false),
        format!("uci: section host_{tag} and GW-{tag}")
    );
}

#[test]
fn longer_hex_identifiers_become_tokens() {
    let out = plain().line(
        "client-id 00:01:00:01:2a:3b:4c:5d:00:11:22:33:44:55 seen",
        false,
    );
    assert!(
        out.starts_with("client-id <id-") && out.ends_with("> seen"),
        "{out}"
    );
}

#[test]
fn router_default_names_are_kept() {
    let redactor = plain();
    for kept in [
        "avahi-daemon[2]: Server startup complete. Host name is router.local. Local service cookie is 1.",
        "avahi-daemon[2]: Host name conflict, retrying with router-2",
        "dnsmasq-dhcp[1]: not giving name router.lan to the DHCP lease of 192.168.0.9 because",
    ] {
        assert_eq!(redactor.line(kept, false), kept);
    }
    let out = redactor.line("Host name is smith-gw.local. Local", false);
    assert!(
        out.contains("Host name is <name-") && !out.contains("smith"),
        "{out}"
    );
}

#[test]
fn emails_are_replaced_outside_the_kernel_log() {
    let redactor = plain();
    let out = redactor.line("mdadm: mail to alice@example.com failed", false);
    assert!(
        out.contains("<user-") && out.contains("@example.com failed") && !out.contains("alice"),
        "{out}"
    );
    for kept in [
        "kern.notice kernel: [    0.000000] Linux version 6.18.57 (builder@build.example) (gcc 14.3.0)",
        "kern.info kernel: [    2.1] device-mapper: ioctl: 4.50.0-ioctl initialised: dm-devel@lists.example.org",
    ] {
        assert_eq!(redactor.line(kept, false), kept);
    }
}

#[test]
fn a_log_keeps_its_lines_and_finds_activity_entries() {
    let redactor = configured();
    let mut log = b"a 84.12.34.56\n".to_vec();
    log.extend_from_slice(b"\xff bad utf8\n");
    log.extend_from_slice(
        b"startwrt-ctrld: startwrt-activity [profiles.created] OK Created profile 'Kids'",
    );
    let out = String::from_utf8(redactor.log(&log)).unwrap();
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(lines[0].starts_with("a <ip4-"), "{out}");
    assert!(
        lines[2].ends_with("Created profile '<profile-voorv>'"),
        "{out}"
    );
}

#[test]
fn zero_runs_compress_after_a_replaced_prefix() {
    assert_eq!(compressed_tail(&[0x1200, 0, 0, 0, 0]), ":1200::");
    assert_eq!(compressed_tail(&[0, 0, 0, 0, 1]), "::1");
    assert_eq!(compressed_tail(&[0, 0, 0, 0, 0]), "::");
    assert_eq!(compressed_tail(&[1, 0, 2, 0, 3]), ":1:0:2:0:3");
}

#[test]
fn short_or_bare_configured_names_are_not_users() {
    let redactor = redactor(
        &[
            ("mdadm", "config mdadm\n\toption email root\n"),
            ("ddns", "config service 'x'\n\toption username 'bob'\n"),
        ],
        &[],
    );
    let line = "dropbear[5]: Exit (root) from <192.168.0.2:5555>: bob disconnected";
    assert_eq!(redactor.line(line, false), line);
}

#[test]
fn a_log_split_into_chunks_is_redacted_whole() {
    let log: String = (0..10)
        .map(|n| format!("line {n} from 84.12.34.{n}\n"))
        .collect();
    let chunks = line_chunks(log.as_bytes(), 3);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), log.as_bytes());
    assert!(chunks.iter().all(|chunk| chunk.ends_with(b"\n")));
    let redactor = plain();
    assert_eq!(redactor.log(log.as_bytes()), redactor.lines(log.as_bytes()));
    assert_eq!(line_chunks(b"", 4), Vec::<&[u8]>::new());
    assert_eq!(line_chunks(b"no newline", 4), vec![&b"no newline"[..]]);
}
