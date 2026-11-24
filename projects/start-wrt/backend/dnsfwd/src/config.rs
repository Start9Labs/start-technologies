use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use serde::Deserialize;

/// One forwarder instance: where it listens and the servers that it asks.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    /// Network device that every upstream socket is bound to.
    #[serde(default)]
    pub interface: Option<String>,
    pub method: Method,
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Fastest,
    InOrder,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Plain,
    Tls,
    Https,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub transport: Transport,
    pub address: IpAddr,
    #[serde(default)]
    pub port: Option<u16>,
    /// Name that the server's certificate must carry.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    pub timeout_ms: u64,
}

impl Server {
    pub fn socket_addr(&self) -> SocketAddr {
        let port = self.port.unwrap_or(match self.transport {
            Transport::Plain => 53,
            Transport::Tls => 853,
            Transport::Https => 443,
        });
        SocketAddr::new(self.address, port)
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    pub fn path(&self) -> &str {
        self.path.as_deref().unwrap_or("/dns-query")
    }
}

impl Config {
    pub fn parse(json: &str) -> Result<Self, String> {
        let config: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.servers.is_empty() {
            return Err("no servers".into());
        }
        if let Some(interface) = &self.interface {
            // IFNAMSIZ less its terminating NUL.
            if interface.is_empty() || interface.len() > 15 {
                return Err(format!("invalid interface name {interface:?}"));
            }
        }
        for server in &self.servers {
            let addr = server.socket_addr();
            if !(1..=60_000).contains(&server.timeout_ms) {
                return Err(format!("{addr}: timeout_ms must be 1 to 60000"));
            }
            match server.transport {
                Transport::Plain if server.name.is_some() || server.path.is_some() => {
                    return Err(format!("{addr}: plain servers take no name or path"));
                }
                Transport::Tls if server.path.is_some() => {
                    return Err(format!("{addr}: TLS servers take no path"));
                }
                Transport::Tls | Transport::Https
                    if server.name.as_deref().is_none_or(str::is_empty) =>
                {
                    return Err(format!("{addr}: TLS and HTTPS servers need a name"));
                }
                Transport::Https if !server.path().starts_with('/') => {
                    return Err(format!("{addr}: path must start with /"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_transport_with_default_ports() {
        let config = Config::parse(
            r#"{
                "listen": "127.0.0.1:5301",
                "interface": "wg0",
                "method": "in_order",
                "servers": [
                    { "transport": "plain", "address": "1.1.1.1", "timeout_ms": 2000 },
                    { "transport": "tls", "address": "9.9.9.9", "name": "dns.quad9.net", "timeout_ms": 3000 },
                    { "transport": "https", "address": "2606:4700:4700::1111", "name": "cloudflare-dns.com", "timeout_ms": 3000 }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(config.method, Method::InOrder);
        assert_eq!(config.interface.as_deref(), Some("wg0"));
        let ports: Vec<_> = config
            .servers
            .iter()
            .map(|s| s.socket_addr().port())
            .collect();
        assert_eq!(ports, [53, 853, 443]);
        assert_eq!(config.servers[2].path(), "/dns-query");
    }

    #[test]
    fn rejects_invalid_configs() {
        let config = |method: &str, servers: &str| {
            Config::parse(&format!(
                r#"{{ "listen": "127.0.0.1:5301", "method": "{method}", "servers": [{servers}] }}"#
            ))
        };
        let plain = r#"{ "transport": "plain", "address": "1.1.1.1", "timeout_ms": 2000 }"#;
        assert!(config("random", plain).is_ok());
        for (method, servers) in [
            ("random", ""),
            ("sometimes", plain),
            (
                "random",
                r#"{ "transport": "tls", "address": "9.9.9.9", "timeout_ms": 2000 }"#,
            ),
            (
                "random",
                r#"{ "transport": "https", "address": "1.1.1.1", "name": "", "timeout_ms": 2000 }"#,
            ),
            (
                "random",
                r#"{ "transport": "plain", "address": "1.1.1.1", "name": "x", "timeout_ms": 2000 }"#,
            ),
            (
                "random",
                r#"{ "transport": "tls", "address": "9.9.9.9", "name": "x", "path": "/q", "timeout_ms": 2000 }"#,
            ),
            (
                "random",
                r#"{ "transport": "https", "address": "1.1.1.1", "name": "x", "path": "q", "timeout_ms": 2000 }"#,
            ),
            (
                "random",
                r#"{ "transport": "plain", "address": "1.1.1.1", "timeout_ms": 0 }"#,
            ),
            (
                "random",
                r#"{ "transport": "plain", "address": "1.1.1.1", "timeout_ms": 2000, "extra": 1 }"#,
            ),
        ] {
            assert!(config(method, servers).is_err(), "{method} {servers}");
        }
        let long_interface = Config::parse(&format!(
            r#"{{ "listen": "127.0.0.1:5301", "interface": "{}", "method": "random", "servers": [{plain}] }}"#,
            "x".repeat(16)
        ));
        assert!(long_interface.is_err());
    }
}
