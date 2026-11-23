//! StartOS's API offered to MCP clients at `/mcp`.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::Request;
use axum::response::Response;
use axum::routing::any;
use http::StatusCode;
use http::uri::Authority;
use imbl_value::imbl::OrdMap;
use rpc_toolkit::HttpServer;
use rpc_toolkit::mcp::rmcp::model::{Implementation, Tool, ToolAnnotations};
use rpc_toolkit::mcp::rmcp::transport::StreamableHttpServerConfig;
use rpc_toolkit::mcp::{McpServer, ToolPolicy};

use crate::context::RpcContext;
use crate::middleware::auth::signature::request_identities;
use crate::net::http::request_authority;
use crate::net::static_server::server_error;
use crate::prelude::*;

/// Handler metadata key. `false` withholds the method from MCP clients.
pub const MCP: &str = "mcp";

/// What a method does to the server. A read grant admits `Read` methods alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Read,
    Write,
    /// Loses data, or takes the server or a service out of the user's reach.
    Destructive,
}

/// Every cataloged method's effect.
const EFFECTS: &[(&str, Effect)] = &[
    ("auth.get-pubkey", Effect::Read),
    ("auth.key.add", Effect::Write),
    ("auth.key.list", Effect::Read),
    ("auth.key.remove", Effect::Write),
    ("auth.login", Effect::Write),
    ("auth.logout", Effect::Write),
    ("auth.reset-password", Effect::Destructive),
    ("auth.token.create", Effect::Write),
    ("auth.token.list", Effect::Read),
    ("auth.token.remove", Effect::Write),
    ("backup.create", Effect::Write),
    ("backup.history.list", Effect::Read),
    ("backup.target.cifs.add", Effect::Write),
    ("backup.target.cifs.remove", Effect::Write),
    ("backup.target.cifs.update", Effect::Write),
    ("backup.target.convert", Effect::Destructive),
    ("backup.target.create", Effect::Write),
    ("backup.target.delete-legacy", Effect::Destructive),
    ("backup.target.find", Effect::Read),
    ("backup.target.info", Effect::Read),
    ("backup.target.inspect", Effect::Read),
    ("backup.target.list", Effect::Read),
    ("backup.target.mount", Effect::Write),
    ("backup.target.remove", Effect::Destructive),
    ("backup.target.rename", Effect::Write),
    ("backup.target.umount", Effect::Write),
    ("backup.target.update-cifs", Effect::Write),
    ("backup.volume.restore", Effect::Destructive),
    ("db.apply", Effect::Destructive),
    ("db.dump", Effect::Read),
    ("db.put.ui", Effect::Write),
    ("db.subscribe", Effect::Read),
    ("diagnostic.disk.forget", Effect::Destructive),
    ("diagnostic.disk.repair", Effect::Destructive),
    ("disk.format", Effect::Destructive),
    ("disk.repair", Effect::Destructive),
    ("echo", Effect::Read),
    ("git-info", Effect::Read),
    ("kiosk.disable", Effect::Write),
    ("kiosk.enable", Effect::Write),
    ("net.acme.check-challenge", Effect::Read),
    ("net.acme.init", Effect::Write),
    ("net.acme.remove", Effect::Write),
    ("net.dns.dump-table", Effect::Read),
    ("net.dns.query", Effect::Read),
    ("net.dns.set-static", Effect::Write),
    ("net.dns.status", Effect::Read),
    ("net.forward.dump-table", Effect::Read),
    ("net.gateway.check-dns", Effect::Read),
    ("net.gateway.check-port", Effect::Read),
    ("net.gateway.check-port-v6", Effect::Read),
    ("net.gateway.edit", Effect::Write),
    ("net.gateway.forget", Effect::Write),
    ("net.gateway.list", Effect::Read),
    ("net.gateway.set-default-outbound", Effect::Write),
    ("net.gateway.set-name", Effect::Write),
    ("net.gateway.set-secure", Effect::Write),
    ("net.gateway.unset-secure", Effect::Write),
    ("net.ssl.generate-certificate", Effect::Write),
    ("net.tunnel.add", Effect::Write),
    ("net.tunnel.remove", Effect::Write),
    ("net.tunnel.update", Effect::Write),
    ("net.vhost.add-passthrough", Effect::Write),
    ("net.vhost.dump-table", Effect::Read),
    ("net.vhost.list-passthrough", Effect::Read),
    ("net.vhost.remove-passthrough", Effect::Write),
    ("notification.create", Effect::Write),
    ("notification.list", Effect::Read),
    ("notification.mark-seen", Effect::Write),
    ("notification.mark-seen-before", Effect::Write),
    ("notification.mark-unseen", Effect::Write),
    ("notification.push.key", Effect::Read),
    ("notification.push.list", Effect::Read),
    ("notification.push.subscribe", Effect::Write),
    ("notification.push.test", Effect::Write),
    ("notification.push.unsubscribe", Effect::Write),
    ("notification.remove", Effect::Write),
    ("notification.remove-before", Effect::Write),
    ("package.action.clear-task", Effect::Write),
    ("package.action.get-input", Effect::Read),
    ("package.action.run", Effect::Destructive),
    ("package.attach", Effect::Destructive),
    ("package.backup.restore", Effect::Destructive),
    ("package.cancel-install", Effect::Write),
    ("package.exec", Effect::Destructive),
    ("package.host.address.domain.private.add", Effect::Write),
    ("package.host.address.domain.private.remove", Effect::Write),
    ("package.host.address.domain.public.add", Effect::Write),
    ("package.host.address.domain.public.remove", Effect::Write),
    ("package.host.address.list", Effect::Read),
    ("package.host.binding.list", Effect::Read),
    ("package.host.binding.reassign-port", Effect::Write),
    ("package.host.binding.set-address-enabled", Effect::Write),
    ("package.host.binding.set-gua-wan", Effect::Write),
    (
        "package.host.binding.set-range-address-enabled",
        Effect::Write,
    ),
    ("package.host.list", Effect::Read),
    ("package.install", Effect::Write),
    ("package.installed-version", Effect::Read),
    ("package.list", Effect::Read),
    ("package.logs", Effect::Read),
    ("package.logs.follow", Effect::Read),
    ("package.rebuild", Effect::Write),
    ("package.restart", Effect::Write),
    ("package.set-outbound-gateway", Effect::Write),
    ("package.sideload", Effect::Write),
    ("package.start", Effect::Write),
    ("package.stats", Effect::Read),
    ("package.stop", Effect::Write),
    ("package.uninstall", Effect::Destructive),
    ("server.clear-nut", Effect::Write),
    ("server.clear-smtp", Effect::Write),
    ("server.device-info", Effect::Read),
    ("server.epp", Effect::Write),
    ("server.experimental.zram", Effect::Write),
    ("server.governor", Effect::Write),
    ("server.host.address.domain.private.add", Effect::Write),
    ("server.host.address.domain.private.remove", Effect::Write),
    ("server.host.address.domain.public.add", Effect::Write),
    ("server.host.address.domain.public.remove", Effect::Write),
    ("server.host.address.list", Effect::Read),
    ("server.host.binding.list", Effect::Read),
    ("server.host.binding.reassign-port", Effect::Write),
    ("server.host.binding.set-address-enabled", Effect::Write),
    ("server.host.binding.set-gua-wan", Effect::Write),
    (
        "server.host.binding.set-range-address-enabled",
        Effect::Write,
    ),
    ("server.kernel-logs", Effect::Read),
    ("server.kernel-logs.follow", Effect::Read),
    ("server.logs", Effect::Read),
    ("server.logs.follow", Effect::Read),
    ("server.metrics", Effect::Read),
    ("server.metrics.follow", Effect::Read),
    ("server.nut-list-ups", Effect::Read),
    ("server.nut-scan", Effect::Read),
    ("server.nut-status", Effect::Read),
    ("server.rebuild", Effect::Destructive),
    ("server.registries.add", Effect::Write),
    ("server.registries.list", Effect::Read),
    ("server.registries.refresh", Effect::Write),
    ("server.registries.remove", Effect::Write),
    ("server.registries.set-os", Effect::Write),
    ("server.restart", Effect::Destructive),
    ("server.set-echoip-urls", Effect::Write),
    ("server.set-hostname", Effect::Write),
    ("server.set-keyboard", Effect::Write),
    ("server.set-language", Effect::Write),
    ("server.set-log-level", Effect::Write),
    ("server.set-nut", Effect::Write),
    ("server.set-smtp", Effect::Write),
    ("server.shutdown", Effect::Destructive),
    ("server.test-smtp", Effect::Write),
    ("server.time", Effect::Read),
    ("server.trust-ca", Effect::Write),
    ("server.trusted-keys.add", Effect::Write),
    ("server.trusted-keys.list", Effect::Read),
    ("server.trusted-keys.remove", Effect::Write),
    ("server.trusted-keys.remove-many", Effect::Write),
    ("server.trusted-keys.update", Effect::Write),
    ("server.update", Effect::Destructive),
    ("server.update-firmware", Effect::Destructive),
    ("ssh.add", Effect::Write),
    ("ssh.list", Effect::Read),
    ("ssh.remove", Effect::Write),
    ("state", Effect::Read),
    ("volumes.create", Effect::Write),
    ("volumes.inspect", Effect::Read),
    ("volumes.remove", Effect::Destructive),
    ("volumes.rename", Effect::Write),
    ("volumes.set-grant", Effect::Write),
    ("volumes.update-cifs", Effect::Write),
    ("wifi.add", Effect::Write),
    ("wifi.available.get", Effect::Read),
    ("wifi.connect", Effect::Write),
    ("wifi.country.set", Effect::Write),
    ("wifi.get", Effect::Read),
    ("wifi.remove", Effect::Write),
    ("wifi.set-enabled", Effect::Write),
];

pub fn effect(method: &str) -> Option<Effect> {
    EFFECTS
        .iter()
        .find(|(name, _)| *name == method)
        .map(|(_, effect)| *effect)
}

/// Generated by `make start-core-mcp-tools`.
const CATALOG: &str = include_str!("../mcp-tools.json");

struct StartOsTools;
impl ToolPolicy for StartOsTools {
    fn expose(&self, _: &str, metadata: &OrdMap<&'static str, Value>) -> bool {
        metadata.get(MCP) != Some(&Value::Bool(false))
    }
    fn annotations(
        &self,
        method: &str,
        _: &OrdMap<&'static str, Value>,
    ) -> Option<ToolAnnotations> {
        effect(method).map(|effect| match effect {
            Effect::Read => ToolAnnotations::new().read_only(true),
            Effect::Write => ToolAnnotations::new().read_only(false).destructive(false),
            Effect::Destructive => ToolAnnotations::new().read_only(false).destructive(true),
        })
    }
}

/// The CLI's translated `about` text, keyed by dotted method name.
fn descriptions() -> BTreeMap<String, String> {
    fn walk(command: &clap::Command, prefix: &str, out: &mut BTreeMap<String, String>) {
        for sub in command.get_subcommands() {
            let path = if prefix.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{prefix}.{}", sub.get_name())
            };
            if let Some(about) = sub.get_about() {
                out.insert(path.clone(), about.to_string());
            }
            walk(sub, &path, out);
        }
    }
    let mut out = BTreeMap::new();
    walk(&crate::bins::start_cli::app().into_command(), "", &mut out);
    out
}

fn server(http: HttpServer<RpcContext>) -> Result<McpServer<RpcContext>, Error> {
    let mut catalog: Vec<Tool> =
        serde_json::from_str(CATALOG).with_kind(ErrorKind::Deserialization)?;
    let mut descriptions = descriptions();
    for tool in &mut catalog {
        tool.description = descriptions.remove(tool.name.as_ref()).map(Into::into);
    }
    McpServer::new(
        http,
        catalog,
        StartOsTools,
        Implementation::new("startos", crate::bins::startos_version()),
    )
    .with_kind(ErrorKind::Incoherent)
}

fn names_host(identities: &[InternedString], authority: Option<&Authority>) -> bool {
    authority.is_some_and(|authority| {
        identities
            .iter()
            .any(|identity| identity.eq_ignore_ascii_case(authority.host()))
    })
}

fn host_is_known<'a>(
    ctx: &'a RpcContext,
    request: &'a Request,
) -> impl Future<Output = Result<bool, Error>> + Send + 'a {
    let authority = request_authority(request);
    let identities = request_identities(ctx, request);
    async move { Ok(names_host(&identities.await?, authority.as_ref())) }
}

fn forbidden() -> Response {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body("Forbidden".into())
        .unwrap()
}

/// Serves `/mcp` for requests addressed to one of the server's own names.
pub fn router(ctx: RpcContext, http: HttpServer<RpcContext>) -> Result<Router, Error> {
    let service =
        server(http)?.service(StreamableHttpServerConfig::default().disable_allowed_hosts());
    Ok(Router::new().route(
        "/mcp",
        any(move |request: Request| {
            let ctx = ctx.clone();
            let service = service.clone();
            async move {
                match host_is_known(&ctx, &request).await {
                    Ok(true) => service.handle(request).await,
                    Ok(false) => forbidden(),
                    Err(e) => server_error(e),
                }
            }
        }),
    ))
}

#[cfg(test)]
mod test {
    use rpc_toolkit::Server;
    use rpc_toolkit::mcp::rmcp::ServerHandler;

    use super::*;

    #[test]
    fn catalog_names_methods_of_the_api() {
        let http = Server::new(
            || async { Err(rpc_toolkit::yajrc::INTERNAL_ERROR) },
            crate::main_api::<RpcContext>(),
        )
        .for_http();
        let server = server(http).unwrap();
        for withheld in [
            "auth.get-pubkey",
            "auth.login",
            "auth.logout",
            "auth.reset-password",
            "auth.key.add",
            "auth.token.create",
            "db.apply",
            "db.put.ui",
            "db.subscribe",
            "package.attach",
            "package.logs.follow",
            "package.sideload",
            "registry.db.subscribe",
            "server.kernel-logs.follow",
            "server.logs.follow",
            "server.metrics.follow",
        ] {
            assert!(server.get_tool(withheld).is_none(), "{withheld} is exposed");
        }
        for exposed in [
            "auth.key.list",
            "auth.key.remove",
            "auth.token.list",
            "auth.token.remove",
            "notification.push.key",
            "notification.push.list",
            "notification.push.subscribe",
            "notification.push.test",
            "notification.push.unsubscribe",
            "package.exec",
            "package.list",
            "package.logs",
            "server.time",
            "volumes.create",
            "volumes.set-grant",
            "backup.volume.restore",
            "notification.push.key",
        ] {
            assert!(server.get_tool(exposed).is_some(), "{exposed} is withheld");
        }
    }

    #[test]
    fn every_cataloged_method_has_one_effect() {
        let catalog: Vec<Tool> = serde_json::from_str(CATALOG).unwrap();
        let names: std::collections::BTreeSet<_> =
            catalog.iter().map(|tool| tool.name.to_string()).collect();
        let classified: std::collections::BTreeSet<_> =
            EFFECTS.iter().map(|(name, _)| name.to_string()).collect();
        assert_eq!(
            classified.len(),
            EFFECTS.len(),
            "a method is classified twice"
        );
        assert_eq!(names, classified);
    }

    #[test]
    fn tools_carry_effects_and_descriptions() {
        let http = Server::new(
            || async { Err(rpc_toolkit::yajrc::INTERNAL_ERROR) },
            crate::main_api::<RpcContext>(),
        )
        .for_http();
        let server = server(http).unwrap();
        for (name, read_only, destructive) in [
            ("package.list", Some(true), None),
            ("package.start", Some(false), Some(false)),
            ("package.exec", Some(false), Some(true)),
            ("package.uninstall", Some(false), Some(true)),
        ] {
            let tool = server.get_tool(name).unwrap();
            let annotations = tool.annotations.unwrap();
            assert_eq!(annotations.read_only_hint, read_only, "{name}");
            assert_eq!(annotations.destructive_hint, destructive, "{name}");
        }
        for name in ["package.list", "package.start", "server.shutdown"] {
            assert!(
                server.get_tool(name).unwrap().description.is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn hosts_match_identities_without_ports_or_case() {
        let identities = [
            "startos.local".into(),
            "[fe80::1]".into(),
            "localhost".into(),
        ];
        for (host, known) in [
            ("startos.local", true),
            ("StartOS.Local:443", true),
            ("[fe80::1]:8443", true),
            ("localhost", true),
            ("evil.example", false),
            ("startos.local.evil.example", false),
        ] {
            let authority = host.parse::<Authority>().unwrap();
            assert_eq!(names_host(&identities, Some(&authority)), known, "{host}");
        }
        assert!(!names_host(&identities, None));
    }
}
