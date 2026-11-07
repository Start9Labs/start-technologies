use std::net::IpAddr;

pub(super) struct DebugData {
    trees: Vec<Tree>,
}

struct Tree {
    unc: Unc,
    health: Option<bool>,
}

struct Unc {
    endpoint: Endpoint,
    share: String,
    has_prepath: bool,
}

#[derive(PartialEq, Eq)]
enum Endpoint {
    Ip(IpAddr),
    Name(String),
}

struct SessionState {
    tcp: u32,
    status: u32,
    disconnected: bool,
    supported: bool,
}

impl DebugData {
    pub(super) fn parse(input: &str) -> Option<Self> {
        let mut lines = input
            .lines()
            .skip_while(|line| !line.trim_start().starts_with("Servers:"));
        let tail = lines.next()?.trim().strip_prefix("Servers:")?.trim();
        let lines: Vec<_> = lines
            .take_while(|line| !line.starts_with("Witness registrations:"))
            .filter(|line| !line.trim().is_empty())
            .collect();
        if tail == "[NONE]" {
            return lines.is_empty().then_some(Self { trees: Vec::new() });
        }
        if !tail.is_empty() {
            return None;
        }
        if lines.len() == 1 && lines[0].trim() == "[NONE]" {
            return Some(Self { trees: Vec::new() });
        }
        let starts: Vec<_> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                line.as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_digit)
                    .then_some(i)
            })
            .collect();
        if starts.first() != Some(&0) {
            return None;
        }
        let mut trees = Vec::new();
        for (i, start) in starts.iter().copied().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(lines.len());
            parse_server(&lines[start..end], i + 1, &mut trees)?;
        }
        Some(Self { trees })
    }

    /// Accepts the raw mountinfo source field, before octal unescaping.
    pub(super) fn health(&self, mounted_unc: &str) -> Option<bool> {
        let mounted = Unc::parse(&unescape_mountinfo(mounted_unc)?)?;
        let mut result = None;
        for tree in &self.trees {
            if tree.unc.endpoint != mounted.endpoint || tree.unc.share != mounted.share {
                continue;
            }
            let health = tree.health?;
            if result.is_some_and(|previous| previous != health) {
                return None;
            }
            result = Some(health);
        }
        result
    }
}

fn parse_server(lines: &[&str], expected: usize, trees: &mut Vec<Tree>) -> Option<()> {
    let (number, header) = numbered(lines.first()?)?;
    if number != expected {
        return None;
    }
    let id = header
        .strip_prefix("ConnectionId: 0x")?
        .split_whitespace()
        .next()?;
    u64::from_str_radix(id, 16).ok()?;
    let sessions = lines.iter().position(|line| line.trim() == "Sessions:")?;
    let mut tcp = None;
    let mut supported = true;
    for line in &lines[1..sessions] {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("TCP status: ") {
            if tcp.is_some() || !value.contains(" Instance: ") {
                return None;
            }
            tcp = Some(decimal(value)?.0);
        }
        if line.starts_with("DFS leaf full path:")
            || line.starts_with("RDMA")
            || line.starts_with("SMBDirect")
        {
            supported = false;
        }
    }
    let tcp = tcp?;
    let sessions = &lines[sessions + 1..];
    if sessions.len() == 1 && sessions[0].trim() == "[NONE]" {
        return Some(());
    }
    if sessions.is_empty() {
        return None;
    }
    let mut start = 0;
    let mut count = 0;
    for (end, line) in sessions.iter().enumerate() {
        if line.trim() == "--" {
            count += 1;
            parse_session(&sessions[start..end], count, tcp, supported, trees)?;
            start = end + 1;
        }
    }
    (start == sessions.len()).then_some(())
}

fn parse_session(
    lines: &[&str],
    expected: usize,
    tcp: u32,
    server_supported: bool,
    trees: &mut Vec<Tree>,
) -> Option<()> {
    let (number, header) = numbered(lines.first()?.trim_start())?;
    if number != expected {
        return None;
    }
    let legacy = header.starts_with("Name: ") && header.contains("  Domain: ");
    let mut status = None;
    let mut disconnected = false;
    if !legacy {
        if !header.starts_with("Address: ") || !header.contains(" Uses: ") {
            return None;
        }
        let (_, value) = header.split_once("Session Status: ")?;
        status = Some(decimal(value)?.0);
        disconnected |= value.contains("password no longer valid");
    }
    let shares = lines.iter().position(|line| line.trim() == "Shares:")?;
    let mut supported = server_supported;
    for line in &lines[1..shares] {
        let line = line.trim();
        if numbered(line).is_some() || line == "Sessions:" {
            return None;
        }
        if let Some(value) = line.strip_prefix("SMB session status: ") {
            if !legacy || status.is_some() {
                return None;
            }
            status = Some(decimal(value)?.0);
            disconnected |= value.contains("password no longer valid");
        }
        if line.starts_with("User:")
            || line.starts_with("Primary channel:")
            || line.starts_with("[RECONNECTING]")
        {
            disconnected |= has_disconnect_flag(line);
        }
        if line.starts_with("Extra Channels:") || line.starts_with("DFS root session id:") {
            supported = false;
        }
    }
    let state = SessionState {
        tcp,
        status: status?,
        disconnected,
        supported,
    };
    let shares = &lines[shares + 1..];
    if shares.iter().any(|line| {
        let line = line.trim();
        line == "Shares:"
            || numbered(line).is_some_and(|(_, body)| {
                body.starts_with("Address: ") || body.starts_with("Name: ")
            })
    }) {
        return None;
    }
    let end = shares
        .iter()
        .position(|line| {
            let line = line.trim();
            line.starts_with("Server interfaces:") || line.starts_with("MIDs:")
        })
        .unwrap_or(shares.len());
    let shares = &shares[..end];
    let starts: Vec<_> = shares
        .iter()
        .enumerate()
        .filter_map(|(i, line)| numbered(line.trim_start()).map(|_| i))
        .collect();
    if starts.first() != Some(&0) {
        return None;
    }
    for (expected, start) in starts.iter().copied().enumerate() {
        let end = starts.get(expected + 1).copied().unwrap_or(shares.len());
        let (number, body) = numbered(shares[start].trim_start())?;
        if number != expected {
            return None;
        }
        let body = if number == 0 {
            body.strip_prefix("IPC: ")?
        } else {
            body
        };
        if number == 0 && body == "none" {
            if end != start + 1 {
                return None;
            }
            continue;
        }
        let tree = parse_tree(body, &shares[start + 1..end], &state)?;
        if number != 0 {
            trees.push(tree);
        }
    }
    Some(())
}

fn parse_tree(header: &str, lines: &[&str], state: &SessionState) -> Option<Tree> {
    let (unc, mounts) = header.rsplit_once(" Mounts: ")?;
    decimal(mounts)?;
    let unc = Unc::parse(unc)?;
    let mut status = None;
    let mut disconnected = false;
    let mut supported = state.supported && !unc.has_prepath;
    for line in lines {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("PathComponentMax: ") {
            if status.is_some() {
                return None;
            }
            let (max, value) = value.split_once(" Status: ")?;
            max.parse::<u32>().ok()?;
            let (value, tail) = decimal(value)?;
            let tail = tail.strip_prefix(" type: ")?;
            let (_, serial) = tail.split_once(" Serial Number: 0x")?;
            u32::from_str_radix(serial.split_whitespace().next()?, 16).ok()?;
            status = Some(value);
            disconnected |= has_disconnect_flag(tail);
        } else if line.starts_with("DFS origin fullpath:") {
            supported = false;
        } else {
            return None;
        }
    }
    let status = status?;
    let health = if !supported || state.tcp > 5 || state.status > 4 || status > 7 {
        None
    } else {
        Some(
            !state.disconnected
                && !disconnected
                && state.tcp == 1
                && state.status == 1
                && status == 1,
        )
    };
    Some(Tree { unc, health })
}

fn numbered(line: &str) -> Option<(usize, &str)> {
    let (number, body) = line.split_once(") ")?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((number.parse().ok()?, body))
}

fn decimal(input: &str) -> Option<(u32, &str)> {
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    let number = &input[..end];
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((number.parse().ok()?, &input[end..]))
}

fn has_disconnect_flag(input: &str) -> bool {
    input
        .split_whitespace()
        .any(|word| matches!(word, "DISCONNECTED" | "[RECONNECTING]"))
}

impl Unc {
    fn parse(input: &str) -> Option<Self> {
        if input.chars().any(char::is_control) {
            return None;
        }
        let normalized = input.replace('\\', "/");
        let mut parts = normalized.strip_prefix("//")?.split('/');
        let host = parts.next()?;
        let share = parts.next()?;
        if host.is_empty()
            || host.chars().any(char::is_whitespace)
            || host.contains('%')
            || share.is_empty()
            || matches!(share, "." | "..")
        {
            return None;
        }
        let endpoint = if host.starts_with('[') || host.ends_with(']') {
            Endpoint::Ip(host.strip_prefix('[')?.strip_suffix(']')?.parse().ok()?)
        } else if let Ok(ip) = host.parse::<IpAddr>() {
            Endpoint::Ip(ip)
        } else {
            Endpoint::Name(host.to_ascii_lowercase())
        };
        let mut has_prepath = false;
        for part in parts {
            if part.is_empty() || matches!(part, "." | "..") {
                return None;
            }
            has_prepath = true;
        }
        Some(Self {
            endpoint,
            share: share.to_owned(),
            has_prepath,
        })
    }
}

fn unescape_mountinfo(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = match bytes.get(index..index + 4) {
            Some(b"\\040") => Some(b' '),
            Some(b"\\011") => Some(b'\t'),
            Some(b"\\012") => Some(b'\n'),
            Some(b"\\134") => Some(b'\\'),
            _ => None,
        };
        if let Some(byte) = decoded {
            output.push(byte);
            index += 4;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

#[cfg(test)]
mod tests {
    use super::DebugData;

    const HEADER: &str = "CIFS Version 2.52\nFeatures: DFS,FSCACHE,ACL\nActive VFS Requests: 0\nServers:\n1) ConnectionId: 0x1 Hostname: 192.0.2.10\nClientGUID: 11111111-1111-1111-1111-111111111111\nNumber of credits: 32,1,1 Dialect 0x311\nTCP status: 1 Instance: 1\nLocal Users To Server: 1 SecMode: 0x1 Req On Wire: 0\n\tSessions:\n";

    const SESSION: &str = r#"	1) Address: 192.0.2.10 Uses: 1 Capability: 0x300047	Session Status: 1
	Security type: RawNTLMSSP  SessionId: 0x1
	User: 0 Cred User: 0
	Shares:
	0) IPC: \\192.0.2.10\IPC$ Mounts: 1 DevInfo: 0x0 Attributes: 0x0
	PathComponentMax: 255 Status: 1 type: 17 Serial Number: 0x0
	1) \\192.0.2.10\Team Files Mounts: 1 Type: NTFS DevInfo: 0x20 Attributes: 0x500ff
	PathComponentMax: 255 Status: 1 type: DISK Serial Number: 0x123 encrypted nocase
	2) \\192.0.2.10\Archive Mounts: 1 DevInfo: 0x20 Attributes: 0x0
	PathComponentMax: 255 Status: 3 type: DISK Serial Number: 0x456	DISCONNECTED
	Server interfaces: 1	Last updated: 0 seconds ago
	1) Speed: 1Gbps
		IPv4: 192.0.2.10
		[CONNECTED]
	MIDs:
	Server ConnectionId: 0x1
		State: 2 com: 5 pid: 1 cbdata: 0000000000000000 mid 3
--
"#;

    fn fixture() -> String {
        format!("{HEADER}{SESSION}")
    }

    fn observe(input: &str, unc: &str) -> Option<bool> {
        DebugData::parse(input).and_then(|data| data.health(unc))
    }

    #[test]
    fn real_address_layout_preserves_tree_scope_and_ignores_ipc_interfaces_and_mids() {
        let data = DebugData::parse(&fixture()).unwrap();
        assert_eq!(data.health("//192.0.2.10/Team Files"), Some(true));
        assert_eq!(data.health("//192.0.2.10/Archive"), Some(false));
        assert_eq!(data.health("//192.0.2.10/IPC$"), None);
        assert_eq!(data.health("//192.0.2.11/Team Files"), None);
        assert_eq!(data.health("//192.0.2.10/Missing"), None);
    }

    #[test]
    fn mounted_unc_handles_mountinfo_escaping_and_prepath() {
        let data = DebugData::parse(&fixture()).unwrap();
        assert_eq!(
            data.health(r"//192.0.2.10/Team\040Files/backups/2026"),
            Some(true),
        );
        assert_eq!(
            data.health(r"\134\134192.0.2.10\134Team\040Files\134backups"),
            Some(true),
        );
        assert_eq!(data.health(r"\\192.0.2.10\Team Files"), Some(true));
        assert_eq!(data.health("//192.0.2.10/Team FilesExtra"), None);
        assert_eq!(data.health("//192.0.2.10/Team/Files"), None);
        for malformed in [
            "192.0.2.10/Team Files",
            "///192.0.2.10/Team Files",
            "//192.0.2.10/",
            "//192.0.2.10/Team Files/../Archive",
            r"//192.0.2.10/Team\011Files",
            r"//192.0.2.10/Team\012Files",
        ] {
            assert_eq!(data.health(malformed), None);
        }
        let delimiter = fixture().replace("Team Files Mounts:", "Team Mounts: 9 Files Mounts:");
        assert_eq!(
            observe(&delimiter, "//192.0.2.10/Team Mounts: 9 Files/backups"),
            Some(true),
        );
    }

    #[test]
    fn real_name_nos_layout_inherits_session_state_and_expired_password() {
        let session = r#"	1) Name: 192.0.2.10  Domain: WORKGROUP Uses: 1 OS: Unix
	NOS: Samba	Capability: 0x0
	SMB session status: 1
	Security type: RawNTLMSSP  SessionId: 0x1
	User: 0 Cred User: 0
	Shares:
	0) IPC: none
	1) \\192.0.2.10\Team Files Mounts: 1 DevInfo: 0x0 Attributes: 0x0
	PathComponentMax: 255 Status: 1 type: DISK Serial Number: 0x1
--
"#;
        let input = format!("{HEADER}{session}");
        assert_eq!(observe(&input, "//192.0.2.10/Team Files"), Some(true));
        let expired = input.replace(
            "SMB session status: 1",
            "SMB session status: 1 password no longer valid",
        );
        assert_eq!(observe(&expired, "//192.0.2.10/Team Files"), Some(false));
        let reconnect = input.replace("SMB session status: 1", "SMB session status: 3");
        assert_eq!(observe(&reconnect, "//192.0.2.10/Team Files"), Some(false));
    }

    #[test]
    fn canonical_ipv6_and_exact_hostname_correlation_do_not_resolve_dns() {
        let ipv6 = fixture().replace("192.0.2.10", "2001:db8::10");
        assert_eq!(
            observe(&ipv6, "//[2001:0db8:0:0::10]/Team Files"),
            Some(true)
        );
        assert_eq!(
            observe(&ipv6, "//2001:db8::10/Team Files/subdir"),
            Some(true)
        );
        assert_eq!(observe(&ipv6, "//[fe80::10%2]/Team Files"), None);
        let named = fixture().replace("192.0.2.10", "NAS.example");
        assert_eq!(observe(&named, "//nas.EXAMPLE/Team Files"), Some(true));
        assert_eq!(observe(&named, "//192.0.2.10/Team Files"), None);
        assert_eq!(observe(&named, "//nas/Team Files"), None);
    }

    #[test]
    fn every_known_non_good_state_is_down() {
        for status in [0, 2, 3, 4, 5] {
            let input = fixture().replace("TCP status: 1", &format!("TCP status: {status}"));
            assert_eq!(observe(&input, "//192.0.2.10/Team Files"), Some(false));
        }
        for status in [0, 2, 3, 4] {
            let input =
                fixture().replace("Session Status: 1", &format!("Session Status: {status}"));
            assert_eq!(observe(&input, "//192.0.2.10/Team Files"), Some(false));
        }
        for status in [0, 2, 3, 4, 5, 6, 7] {
            let input = fixture().replace(
                "Status: 1 type: DISK",
                &format!("Status: {status} type: DISK"),
            );
            assert_eq!(observe(&input, "//192.0.2.10/Team Files"), Some(false));
        }
    }

    #[test]
    fn flags_override_good_numbers_without_contaminating_siblings() {
        for flag in ["Primary channel: DISCONNECTED", "[RECONNECTING]"] {
            let input = fixture().replace(
                "User: 0 Cred User: 0",
                &format!("User: 0 Cred User: 0\t{flag}"),
            );
            assert_eq!(observe(&input, "//192.0.2.10/Team Files"), Some(false));
        }
        let expired = fixture().replace(
            "Session Status: 1",
            "Session Status: 1 password no longer valid",
        );
        assert_eq!(observe(&expired, "//192.0.2.10/Team Files"), Some(false));
        let tree = fixture().replace(
            "0x123 encrypted nocase",
            "0x123 encrypted nocase DISCONNECTED",
        );
        assert_eq!(observe(&tree, "//192.0.2.10/Team Files"), Some(false));
        let name = fixture().replace("Team Files Mounts:", "DISCONNECTED Mounts:");
        assert_eq!(observe(&name, "//192.0.2.10/DISCONNECTED"), Some(true));
    }

    #[test]
    fn unknown_states_never_claim_up_or_override_unsupported_with_down() {
        for (from, to, archive) in [
            ("TCP status: 1", "TCP status: 6", None),
            ("Session Status: 1", "Session Status: 5", None),
            ("Status: 1 type: DISK", "Status: 8 type: DISK", Some(false)),
        ] {
            let input = fixture().replace(from, to);
            assert_eq!(observe(&input, "//192.0.2.10/Team Files"), None);
            assert_eq!(observe(&input, "//192.0.2.10/Archive"), archive);
        }
    }

    #[test]
    fn unsupported_multichannel_dfs_and_rdma_use_fallback() {
        let channels = fixture().replace(
            "\tShares:",
            "\tExtra Channels: 1\n\t\tChannel: 2 ConnectionId: 0x2\n\t\tTCP status: 3 Instance: 1\n\t\tDISCONNECTED [RECONNECTING]\n\tShares:",
        );
        assert_eq!(observe(&channels, "//192.0.2.10/Team Files"), None);
        let channels_up = channels
            .replace("TCP status: 3", "TCP status: 1")
            .replace("DISCONNECTED [RECONNECTING]", "");
        assert_eq!(observe(&channels_up, "//192.0.2.10/Team Files"), None);
        let dfs_server = fixture().replace(
            "\tSessions:",
            "DFS leaf full path: \\domain\\root\n\tSessions:",
        );
        assert_eq!(observe(&dfs_server, "//192.0.2.10/Team Files"), None);
        let dfs_tree = fixture().replace(
            "0x123 encrypted nocase",
            "0x123 encrypted nocase\n\tDFS origin fullpath: \\domain\\root",
        );
        assert_eq!(observe(&dfs_tree, "//192.0.2.10/Team Files"), None);
        assert_eq!(observe(&dfs_tree, "//192.0.2.10/Archive"), Some(false));
        let rdma = fixture().replace("TCP status: 1", "RDMA \nTCP status: 1");
        assert_eq!(observe(&rdma, "//192.0.2.10/Team Files"), None);
    }

    #[test]
    fn malformed_or_truncated_required_structure_is_rejected() {
        for input in [
            String::new(),
            "Servers:\n".to_owned(),
            fixture().replace("ConnectionId:", "UnsupportedConnectionId:"),
            fixture().replace(
                "TCP status: 1 Instance: 1",
                "TCP status: unknown Instance: 1",
            ),
            fixture().replace("Session Status: 1", "Session Status: -1"),
            fixture().replace("\tShares:", "\tTrees:"),
            fixture().replace(
                "PathComponentMax: 255 Status: 1 type: DISK Serial Number: 0x123 encrypted nocase",
                "PathComponentMax: 255 Status: 1",
            ),
            fixture().replace("\n--\n", "\n"),
            fixture().replace("\t1) \\\\192.0.2.10\\Team Files", "\t1) Team Files"),
        ] {
            assert!(DebugData::parse(&input).is_none());
        }
    }

    #[test]
    fn absent_servers_sessions_and_non_ipc_trees_are_not_proof_of_failure() {
        for input in [
            "Servers: [NONE]\n".to_owned(),
            "Servers:\n\t[NONE]\nWitness registrations:\n".to_owned(),
            format!("{HEADER}\t\t[NONE]\n"),
            format!(
                "{HEADER}\t1) Address: 192.0.2.10 Uses: 1 Capability: 0x0\tSession Status: 1\n\tShares:\n\t0) IPC: none\n--\n"
            ),
        ] {
            let parsed = DebugData::parse(&input).unwrap();
            assert_eq!(parsed.health("//192.0.2.10/Team Files"), None);
        }
    }

    #[test]
    fn conflicting_duplicate_sessions_and_connections_cannot_select_a_mount() {
        let second = SESSION.replace("\t1) Address:", "\t2) Address:");
        let same = format!("{HEADER}{SESSION}{second}");
        assert_eq!(observe(&same, "//192.0.2.10/Team Files"), Some(true));
        let conflict = format!(
            "{HEADER}{SESSION}{}",
            second.replace("Session Status: 1", "Session Status: 3")
        );
        assert_eq!(observe(&conflict, "//192.0.2.10/Team Files"), None);
        let unknown = format!(
            "{HEADER}{SESSION}{}",
            second.replace("Session Status: 1", "Session Status: 99")
        );
        assert_eq!(observe(&unknown, "//192.0.2.10/Team Files"), None);
        let second_server = HEADER.replace("1) ConnectionId: 0x1", "2) ConnectionId: 0x2");
        let second_server = second_server.split_once("Servers:\n").unwrap().1;
        let connections = format!(
            "{HEADER}{SESSION}{second_server}{}",
            SESSION.replace("Session Status: 1", "Session Status: 3")
        );
        assert_eq!(observe(&connections, "//192.0.2.10/Team Files"), None);
    }

    #[test]
    fn conflicting_duplicate_trees_and_uniform_down_consensus() {
        let duplicate = r#"	3) \\192.0.2.10\Team Files Mounts: 1 DevInfo: 0x0 Attributes: 0x0
	PathComponentMax: 255 Status: 3 type: DISK Serial Number: 0x789
"#;
        let conflict = fixture().replace(
            "\tServer interfaces:",
            &format!("{duplicate}\tServer interfaces:"),
        );
        assert_eq!(observe(&conflict, "//192.0.2.10/Team Files"), None);
        let down = conflict.replace("Status: 1 type: DISK", "Status: 3 type: DISK");
        assert_eq!(observe(&down, "//192.0.2.10/Team Files"), Some(false));
    }
}
