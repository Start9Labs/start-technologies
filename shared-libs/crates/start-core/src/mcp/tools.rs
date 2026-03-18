use std::collections::HashMap;

use serde_json::{Value as JsonValue, json};

use super::protocol::ToolDefinition;

pub struct ToolEntry {
    pub definition: ToolDefinition,
    pub rpc_method: &'static str,
    pub sync_db: bool,
    /// If true, inject `__Auth_session` (hashed session token) into params before dispatch.
    pub needs_session: bool,
}

fn tool(name: &str, description: &str, rpc_method: &'static str) -> ToolEntry {
    ToolEntry {
        definition: ToolDefinition {
            name: name.into(),
            description: description.into(),
            input_schema: empty_schema(),
        },
        rpc_method,
        sync_db: false,
        needs_session: false,
    }
}

fn empty_schema() -> JsonValue {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn pkg_id_schema() -> JsonValue {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Package identifier (e.g. 'bitcoind', 'lnd'). Read or subscribe to the startos:///public/packageData resource to see installed packages." }
        },
        "required": ["id"],
        "additionalProperties": false
    })
}

pub fn tool_registry() -> HashMap<String, ToolEntry> {
    let tools = vec![
        // =====================================================================
        // Server management
        // =====================================================================
        tool(
            "server.time",
            "Get the current server time (UTC) and uptime. Use this to check if the server is responsive and how long it has been running since last reboot.",
            "server.time",
        ),
        tool(
            "server.device-info",
            "Get hardware and software information about this StartOS server, including the device name, server ID, OS version, CPU architecture, platform, and network addresses (LAN, Tor). Use this to understand what device you are managing.",
            "server.device-info",
        ),
        tool(
            "server.metrics",
            "Get current server resource usage: CPU load, memory usage, disk usage, and temperature. Returns null if metrics are not yet available (the server collects metrics periodically). Use this to diagnose performance issues or check available disk space.",
            "server.metrics",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "server.shutdown".into(),
                description: "Shut down the StartOS server. THIS IS DESTRUCTIVE: the server will power off and all services will stop. The server must be physically restarted. Always confirm with the user before calling this.".into(),
                input_schema: empty_schema(),
            },
            rpc_method: "server.shutdown",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.restart".into(),
                description: "Restart the StartOS server. All services will stop and restart automatically. The server will be temporarily unreachable during the reboot. Confirm with the user before calling this.".into(),
                input_schema: empty_schema(),
            },
            rpc_method: "server.restart",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.rebuild".into(),
                description: "Tear down and rebuild all service containers. This restarts every installed service. Use this to fix issues caused by corrupted container state. Services will be temporarily unavailable. Confirm with the user before calling.".into(),
                input_schema: empty_schema(),
            },
            rpc_method: "server.rebuild",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.set-hostname".into(),
                description: "Change the server's display name and/or hostname. The name is a human-readable label. The hostname is the network identifier. Either can be provided independently.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Human-readable display name for the server" },
                        "hostname": { "type": "string", "description": "Network hostname (alphanumeric and hyphens only)" }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.set-hostname",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.update".into(),
                description: "Check for and apply a StartOS system update from a registry. The update downloads and installs asynchronously — subscribe to startos:///public/serverStatus to monitor update progress. THIS WILL REBOOT THE SERVER if an update is applied. Confirm with the user before calling.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "registry": { "type": "string", "description": "URL of the update registry (e.g. 'https://registry.start9.com')" },
                        "target": { "type": "string", "description": "Target version range (optional, uses latest if omitted)" }
                    },
                    "required": ["registry"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.update",
            sync_db: true,
            needs_session: false,
        },
        tool(
            "server.update-firmware",
            "Check for and apply a firmware update for the server hardware. Returns whether a reboot is required. Only applicable on supported hardware platforms.",
            "server.update-firmware",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "server.set-smtp".into(),
                description: "Configure the server's SMTP settings for sending email notifications. Requires SMTP server details.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "host": { "type": "string", "description": "SMTP server hostname" },
                        "port": { "type": "integer", "description": "SMTP server port (e.g. 587 for STARTTLS, 465 for TLS)" },
                        "from": { "type": "string", "description": "Sender email address" },
                        "username": { "type": "string", "description": "SMTP username" },
                        "password": { "type": "string", "description": "SMTP password" },
                        "security": { "type": "string", "enum": ["Starttls", "Tls"], "description": "Connection security. Default: Starttls" }
                    },
                    "required": ["host", "port", "from", "username"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.set-smtp",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.test-smtp".into(),
                description: "Send a test email to verify SMTP settings are working correctly. Does not save the settings.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "host": { "type": "string", "description": "SMTP server hostname" },
                        "port": { "type": "integer", "description": "SMTP server port" },
                        "from": { "type": "string", "description": "Sender email address" },
                        "to": { "type": "string", "description": "Recipient email address for the test" },
                        "username": { "type": "string", "description": "SMTP username" },
                        "password": { "type": "string", "description": "SMTP password" },
                        "security": { "type": "string", "enum": ["Starttls", "Tls"], "description": "Connection security. Default: Starttls" }
                    },
                    "required": ["host", "port", "from", "to", "username", "password"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.test-smtp",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "server.clear-smtp",
            "Remove the server's SMTP configuration. Email notifications will no longer be sent.",
            "server.clear-smtp",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "server.set-echoip-urls".into(),
                description: "Set the URLs used to determine the server's external IP address. These services are queried to detect the WAN IP.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "urls": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of echoip service URLs"
                        }
                    },
                    "required": ["urls"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.set-echoip-urls",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.set-keyboard".into(),
                description: "Set the keyboard layout for the server console (kiosk mode).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "layout": { "type": "string", "description": "Keyboard layout code (e.g. 'us', 'de', 'fr')" },
                        "keymap": { "type": "string", "description": "Keymap name (optional)" },
                        "model": { "type": "string", "description": "Keyboard model (optional)" },
                        "variant": { "type": "string", "description": "Layout variant (optional)" },
                        "options": { "type": "array", "items": { "type": "string" }, "description": "Additional keyboard options (optional)" }
                    },
                    "required": ["layout"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.set-keyboard",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.set-language".into(),
                description: "Set the system locale/language for the server.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "language": { "type": "string", "description": "Language code (e.g. 'en_US', 'de_DE', 'es_ES')" }
                    },
                    "required": ["language"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.set-language",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.experimental.zram".into(),
                description: "Enable or disable ZRAM compressed swap. ZRAM uses compressed RAM as swap space, which can improve performance on memory-constrained systems.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "enable": { "type": "boolean", "description": "true to enable ZRAM, false to disable" }
                    },
                    "required": ["enable"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.experimental.zram",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.experimental.governor".into(),
                description: "Get or set the CPU frequency governor. Returns current governor and available options. Pass 'set' to change the governor.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "set": { "type": "string", "description": "Governor to set (e.g. 'performance', 'powersave', 'ondemand'). Omit to just read current state." }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.experimental.governor",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Package / service management
        // =====================================================================
        tool(
            "package.list",
            "List all installed packages (services) on this StartOS server. Returns each package's ID, version, and current status. For real-time status updates, subscribe to the startos:///public/packageData resource instead of polling this tool.",
            "package.list",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "package.install".into(),
                description: "Install a package (service) from a registry. The installation is asynchronous — it starts the download and setup, but the service will not be immediately available. Subscribe to startos:///public/packageData/<id> to monitor this package's installation progress in real time.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "registry": { "type": "string", "description": "URL of the package registry (e.g. 'https://registry.start9.com')" },
                        "id": { "type": "string", "description": "Package identifier (e.g. 'bitcoind', 'lnd', 'nextcloud')" },
                        "version": { "type": "string", "description": "Version to install (e.g. '0.27.0:0')" }
                    },
                    "required": ["registry", "id", "version"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.install",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.sideload-by-url".into(),
                description: "Install a package (service) from a direct URL to an .s9pk file. \
                    Use this to install packages from download links rather than from a registry. \
                    The download and installation is asynchronous — subscribe to \
                    startos:///public/packageData to monitor installation progress in real time.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "Direct URL to an .s9pk package file (http or https)" }
                    },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.sideload-url",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.uninstall".into(),
                description: "Uninstall a package (service). THIS IS DESTRUCTIVE: the service and its data will be removed unless 'soft' is true. Always confirm with the user.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Package identifier to uninstall" },
                        "soft": { "type": "boolean", "description": "If true, keep package data on disk for potential reinstall. Default: false" },
                        "force": { "type": "boolean", "description": "If true, force uninstall even if other packages depend on this one. Default: false" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.uninstall",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.start".into(),
                description: "Start an installed service. Starting is asynchronous — subscribe to startos:///public/packageData/<id> to monitor the state transition.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.start",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.stop".into(),
                description: "Stop a running service. The service will be gracefully shut down. Subscribe to startos:///public/packageData/<id> to monitor the state transition. It can be restarted later with package.start.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.stop",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.restart".into(),
                description: "Restart a running service. Equivalent to stop followed by start. Subscribe to startos:///public/packageData/<id> to monitor the state transition.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.restart",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.rebuild".into(),
                description: "Rebuild a service's container. Subscribe to startos:///public/packageData/<id> to monitor the rebuild. Use this to fix container corruption for a specific service without affecting others.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.rebuild",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.cancel-install".into(),
                description: "Cancel an in-progress package installation.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.cancel-install",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.installed-version".into(),
                description: "Get the installed version of a specific package. Returns null if the package is not installed.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.installed-version",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "package.stats",
            "Get resource usage statistics for all LXC service containers: memory usage, memory limit, and percentage. Use this to identify which services are using the most resources.",
            "package.stats",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "package.set-outbound-gateway".into(),
                description: "Set the outbound network gateway for a specific package. This controls which network interface the service uses for outbound connections. Set gateway to null to use the system default.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "package": { "type": "string", "description": "Package identifier" },
                        "gateway": { "type": "string", "description": "Gateway ID to use for outbound traffic, or null for system default" }
                    },
                    "required": ["package"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.set-outbound-gateway",
            sync_db: true,
            needs_session: false,
        },
        // === Package actions ===
        ToolEntry {
            definition: ToolDefinition {
                name: "package.action.get-input".into(),
                description: "Get the input specification for a package action. Returns the form schema that describes what input the action expects. Use this before running an action to understand what parameters it needs.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "packageId": { "type": "string", "description": "Package identifier" },
                        "actionId": { "type": "string", "description": "Action identifier" },
                        "prefill": { "type": "object", "description": "Optional prefill values for the action form" }
                    },
                    "required": ["packageId", "actionId"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.action.get-input",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.action.run".into(),
                description: "Run a package action. Actions are service-specific operations defined by each package (e.g. reset password, create user, run maintenance). Use package.action.get-input first to understand what input is needed.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "packageId": { "type": "string", "description": "Package identifier" },
                        "actionId": { "type": "string", "description": "Action identifier" },
                        "input": { "description": "Input data for the action (schema varies per action)" }
                    },
                    "required": ["packageId", "actionId"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.action.run",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.action.clear-task".into(),
                description: "Clear a completed or failed action task for a package.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "packageId": { "type": "string", "description": "Package identifier" },
                        "replayId": { "type": "string", "description": "Replay/task ID to clear" },
                        "force": { "type": "boolean", "description": "Force clear even if task is still running. Default: false" }
                    },
                    "required": ["packageId", "replayId"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.action.clear-task",
            sync_db: false,
            needs_session: false,
        },
        // === Package backup ===
        ToolEntry {
            definition: ToolDefinition {
                name: "package.backup.restore".into(),
                description: "Restore packages from a backup. Requires a mounted backup target with a valid backup.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "targetId": { "type": "string", "description": "Backup target ID (must be mounted)" },
                        "packageIds": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Package IDs to restore. If omitted, all backed-up packages are restored."
                        },
                        "password": { "type": "string", "description": "Backup encryption password" }
                    },
                    "required": ["targetId", "password"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.backup.restore",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Notifications
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "notification.list".into(),
                description: "List recent notifications from the server. Notifications include system events, service status changes, backup results, and errors. Use 'before' to paginate and 'limit' to control page size.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "before": { "type": "integer", "description": "Return notifications with IDs before this value (for pagination)" },
                        "limit": { "type": "integer", "description": "Maximum number of notifications to return" }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "notification.list",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "notification.remove".into(),
                description: "Remove specific notifications by their IDs.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ids": { "type": "array", "items": { "type": "integer" }, "description": "Notification IDs to remove" }
                    },
                    "required": ["ids"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "notification.remove",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "notification.remove-before".into(),
                description: "Remove all notifications with IDs before the specified value.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "before": { "type": "integer", "description": "Remove notifications with IDs less than this value" }
                    },
                    "required": ["before"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "notification.remove-before",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "notification.mark-seen".into(),
                description: "Mark specific notifications as seen/read.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ids": { "type": "array", "items": { "type": "integer" }, "description": "Notification IDs to mark as seen" }
                    },
                    "required": ["ids"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "notification.mark-seen",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "notification.mark-seen-before".into(),
                description: "Mark all notifications with IDs before the specified value as seen/read.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "before": { "type": "integer", "description": "Mark notifications with IDs less than this value as seen" }
                    },
                    "required": ["before"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "notification.mark-seen-before",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "notification.mark-unseen".into(),
                description: "Mark specific notifications as unseen/unread.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ids": { "type": "array", "items": { "type": "integer" }, "description": "Notification IDs to mark as unseen" }
                    },
                    "required": ["ids"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "notification.mark-unseen",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // SSH keys
        // =====================================================================
        tool(
            "ssh.list",
            "List all SSH public keys authorized to access this server. Returns each key's algorithm, fingerprint, hostname label, and creation date.",
            "ssh.list",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "ssh.add".into(),
                description: "Add an SSH public key to the server's authorized keys.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "description": "SSH public key in OpenSSH format (e.g. 'ssh-ed25519 AAAA... user@host')" }
                    },
                    "required": ["key"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "ssh.add",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "ssh.remove".into(),
                description: "Remove an SSH public key from the server's authorized keys. Use ssh.list to find the fingerprint.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "fingerprint": { "type": "string", "description": "Fingerprint of the SSH key to remove" }
                    },
                    "required": ["fingerprint"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "ssh.remove",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Backup
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.create".into(),
                description: "Create a backup of server data and installed packages to a backup target (USB drive, network share). The backup runs asynchronously — subscribe to startos:///public/serverStatus to monitor backup progress in real time.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "targetId": { "type": "string", "description": "Backup target ID. Use backup.target.list to see available targets." },
                        "password": { "type": "string", "description": "Encryption password for the backup" },
                        "oldPassword": { "type": "string", "description": "Previous backup password, if changing password" },
                        "packageIds": { "type": "array", "items": { "type": "string" }, "description": "Specific package IDs to back up. If omitted, all packages are backed up." }
                    },
                    "required": ["targetId", "password"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.create",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "backup.target.list",
            "List all configured backup targets (USB drives, CIFS/network shares). Returns target IDs, types, and connection details.",
            "backup.target.list",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.target.info".into(),
                description: "Get detailed information about a backup on a target, including which packages were backed up and when.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "targetId": { "type": "string", "description": "Backup target ID" },
                        "serverId": { "type": "string", "description": "Server ID to get backup info for" },
                        "password": { "type": "string", "description": "Backup encryption password" }
                    },
                    "required": ["targetId", "serverId", "password"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.target.info",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.target.mount".into(),
                description: "Mount a backup target to make it accessible for backup or restore operations.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "targetId": { "type": "string", "description": "Backup target ID to mount" },
                        "serverId": { "type": "string", "description": "Server ID (optional)" },
                        "password": { "type": "string", "description": "Backup encryption password" },
                        "allowPartial": { "type": "boolean", "description": "Allow mounting even if some data is incomplete. Default: false" }
                    },
                    "required": ["targetId", "password"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.target.mount",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.target.umount".into(),
                description: "Unmount a backup target.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "targetId": { "type": "string", "description": "Backup target ID to unmount. If omitted, unmounts all." }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.target.umount",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.target.cifs.add".into(),
                description: "Add a new CIFS/SMB network backup target (e.g. a NAS share).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "hostname": { "type": "string", "description": "CIFS server hostname or IP" },
                        "path": { "type": "string", "description": "Share path on the server (e.g. '/backups')" },
                        "username": { "type": "string", "description": "CIFS username" },
                        "password": { "type": "string", "description": "CIFS password (optional for guest access)" }
                    },
                    "required": ["hostname", "path", "username"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.target.cifs.add",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.target.cifs.update".into(),
                description: "Update an existing CIFS/SMB network backup target's connection details.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Backup target ID to update" },
                        "hostname": { "type": "string", "description": "CIFS server hostname or IP" },
                        "path": { "type": "string", "description": "Share path on the server" },
                        "username": { "type": "string", "description": "CIFS username" },
                        "password": { "type": "string", "description": "CIFS password" }
                    },
                    "required": ["id", "hostname", "path", "username"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.target.cifs.update",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "backup.target.cifs.remove".into(),
                description: "Remove a CIFS/SMB network backup target.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Backup target ID to remove" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "backup.target.cifs.remove",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Network
        // =====================================================================
        tool(
            "net.gateway.list",
            "List all network gateways (interfaces) the server can listen on. Shows interface type, IP addresses, and WAN IP for each.",
            "net.gateway.list",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "net.gateway.forget".into(),
                description: "Remove a disconnected gateway from the server's list. Only works on gateways that are currently disconnected.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "gateway": { "type": "string", "description": "Gateway ID to forget" }
                    },
                    "required": ["gateway"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.gateway.forget",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.gateway.set-name".into(),
                description: "Rename a network gateway for easier identification in the UI.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Gateway ID to rename" },
                        "name": { "type": "string", "description": "New display name for the gateway" }
                    },
                    "required": ["id", "name"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.gateway.set-name",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.gateway.check-port".into(),
                description: "Check if a port is reachable from the internet through a specific gateway. Tests both external and internal reachability, and hairpinning support.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "port": { "type": "integer", "description": "Port number to check" },
                        "gateway": { "type": "string", "description": "Gateway ID to check the port through" }
                    },
                    "required": ["port", "gateway"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.gateway.check-port",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.gateway.check-dns".into(),
                description: "Check if DNS resolution works through a specific gateway.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "gateway": { "type": "string", "description": "Gateway ID to check DNS through" }
                    },
                    "required": ["gateway"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.gateway.check-dns",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.dns.query".into(),
                description: "Perform a DNS lookup for a fully qualified domain name using the server's DNS resolver.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "fqdn": { "type": "string", "description": "Fully qualified domain name to look up" }
                    },
                    "required": ["fqdn"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.dns.query",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.dns.set-static".into(),
                description: "Set static DNS server addresses. If null/empty, the server uses DHCP-provided DNS.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "servers": { "type": "array", "items": { "type": "string" }, "description": "List of DNS server IP addresses (e.g. ['1.1.1.1', '8.8.8.8'])" }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.dns.set-static",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "net.dns.dump-table",
            "Dump the server's internal DNS resolution table for debugging.",
            "net.dns.dump-table",
        ),
        tool(
            "net.forward.dump-table",
            "Dump the server's port forwarding table for debugging.",
            "net.forward.dump-table",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "net.acme.init".into(),
                description: "Initialize an ACME certificate provider for automated SSL/TLS certificate management (e.g. Let's Encrypt).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "provider": { "type": "string", "description": "ACME provider identifier" },
                        "contact": { "type": "array", "items": { "type": "string" }, "description": "Contact email addresses for the ACME account" }
                    },
                    "required": ["provider", "contact"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.acme.init",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.acme.remove".into(),
                description: "Remove an ACME certificate provider configuration.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "provider": { "type": "string", "description": "ACME provider identifier to remove" }
                    },
                    "required": ["provider"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.acme.remove",
            sync_db: true,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.tunnel.add".into(),
                description: "Add a VPN/tunnel gateway configuration. This allows the server to route traffic through VPN tunnels.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Display name for the tunnel" },
                        "config": { "type": "string", "description": "WireGuard configuration content" },
                        "type": { "type": "string", "description": "Gateway type (optional)" },
                        "setAsDefaultOutbound": { "type": "boolean", "description": "Set this tunnel as the default outbound gateway. Default: false" }
                    },
                    "required": ["config"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.tunnel.add",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.tunnel.remove".into(),
                description: "Remove a VPN/tunnel gateway configuration.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Gateway/tunnel ID to remove" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.tunnel.remove",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "net.vhost.dump-table",
            "Dump the SSL virtual host proxy table for debugging.",
            "net.vhost.dump-table",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "net.vhost.add-passthrough".into(),
                description: "Add an SSL passthrough rule. Incoming TLS connections matching the hostname will be forwarded directly to the backend without termination.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "hostname": { "type": "string", "description": "Hostname to match for passthrough" },
                        "listenPort": { "type": "integer", "description": "Port to listen on" },
                        "backend": { "type": "string", "description": "Backend address (IP:port)" },
                        "publicGateway": { "type": "array", "items": { "type": "string" }, "description": "Gateway IDs to expose on publicly" },
                        "privateIp": { "type": "array", "items": { "type": "string" }, "description": "Private IP addresses to listen on" }
                    },
                    "required": ["hostname", "listenPort", "backend"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.vhost.add-passthrough",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "net.vhost.remove-passthrough".into(),
                description: "Remove an SSL passthrough rule.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "hostname": { "type": "string", "description": "Hostname of the passthrough to remove" },
                        "listenPort": { "type": "integer", "description": "Listen port of the passthrough to remove" }
                    },
                    "required": ["hostname", "listenPort"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "net.vhost.remove-passthrough",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "net.vhost.list-passthrough",
            "List all SSL passthrough rules currently configured.",
            "net.vhost.list-passthrough",
        ),
        // =====================================================================
        // WiFi
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "wifi.set-enabled".into(),
                description: "Enable or disable the WiFi interface on the server.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean", "description": "true to enable WiFi, false to disable" }
                    },
                    "required": ["enabled"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "wifi.set-enabled",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "wifi.add".into(),
                description: "Add a WiFi network configuration (SSID and password). Does not connect immediately — use wifi.connect to connect.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ssid": { "type": "string", "description": "WiFi network name (SSID)" },
                        "password": { "type": "string", "description": "WiFi password" }
                    },
                    "required": ["ssid", "password"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "wifi.add",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "wifi.connect".into(),
                description: "Connect to a previously added WiFi network.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ssid": { "type": "string", "description": "WiFi network name (SSID) to connect to" }
                    },
                    "required": ["ssid"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "wifi.connect",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "wifi.remove".into(),
                description: "Remove a saved WiFi network configuration. If currently connected to this network, the server will disconnect.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ssid": { "type": "string", "description": "WiFi network name (SSID) to remove" }
                    },
                    "required": ["ssid"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "wifi.remove",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "wifi.get".into(),
                description: "Get details about a specific saved WiFi network configuration.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ssid": { "type": "string", "description": "WiFi network name (SSID)" }
                    },
                    "required": ["ssid"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "wifi.get",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "wifi.country.set".into(),
                description: "Set the WiFi regulatory country code. This affects which channels and power levels are available.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "country": { "type": "string", "description": "ISO 3166-1 alpha-2 country code (e.g. 'US', 'DE', 'GB')" }
                    },
                    "required": ["country"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "wifi.country.set",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "wifi.available.get",
            "Scan for and list available WiFi networks in range of the server.",
            "wifi.available.get",
        ),
        // =====================================================================
        // Auth
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "auth.reset-password".into(),
                description: "Change the server's master password. THIS IS SENSITIVE: the master password controls all access to the server. Requires the old password for verification (unless first-time setup).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "oldPassword": { "type": "string", "description": "Current password (required unless first-time setup)" },
                        "newPassword": { "type": "string", "description": "New password to set" }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "auth.reset-password",
            sync_db: false,
            needs_session: false,
        },
        tool(
            "auth.get-pubkey",
            "Get the server's public key (JWK format). This is the server's identity key used for cryptographic verification.",
            "auth.get-pubkey",
        ),
        ToolEntry {
            definition: ToolDefinition {
                name: "auth.session.kill".into(),
                description: "Terminate specific authenticated sessions by their IDs. Use this to revoke access for specific sessions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "ids": { "type": "array", "items": { "type": "string" }, "description": "Session IDs to terminate" }
                    },
                    "required": ["ids"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "auth.session.kill",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Database
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "db.dump".into(),
                description: "Dump the current database state, optionally filtered to a specific path. Use this to inspect the full system state or a specific section. The pointer uses JSON Pointer syntax (e.g. '/public/serverInfo').".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pointer": { "type": "string", "description": "JSON Pointer path to dump (e.g. '/public/serverInfo'). If omitted, dumps the entire database." }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "db.dump",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "db.put.ui".into(),
                description: "Write a value to a specific path in the UI section of the database. This is for advanced use — incorrect values can break the UI state. The pointer uses JSON Pointer syntax.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pointer": { "type": "string", "description": "JSON Pointer path to write to (e.g. '/ui/someSetting')" },
                        "value": { "description": "Value to write at the specified path" }
                    },
                    "required": ["pointer", "value"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "db.put.ui",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Disk
        // =====================================================================
        tool(
            "disk.list",
            "List all disks attached to the server, including their size, partitions, and mount status.",
            "disk.list",
        ),
        tool(
            "disk.repair",
            "Run filesystem repair on the server's data partition. Use this if the server reports filesystem errors. The server may need to restart.",
            "disk.repair",
        ),
        // =====================================================================
        // Server host (network address & binding management for system UI)
        // =====================================================================
        tool(
            "server.host.address.list",
            "List all network addresses configured for the server's system UI. This includes LAN, Tor, and any custom domain addresses.",
            "server.host.address.list",
        ),
        tool(
            "server.host.binding.list",
            "List all network bindings for the server's system UI. Shows which addresses and ports the UI is accessible on.",
            "server.host.binding.list",
        ),
        // =====================================================================
        // Package host (network address & binding management per package)
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "package.host.list".into(),
                description: "List all network host configurations for a specific package.".into(),
                input_schema: pkg_id_schema(),
            },
            rpc_method: "package.host.list",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Logs
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "server.logs".into(),
                description: "Fetch recent OS/system log entries from journald. Returns log lines \
                    with timestamps. Use 'limit' to control how many entries to return (default: \
                    all available). Use 'cursor' to paginate from a specific position. Use 'before' \
                    to get entries before the cursor instead of after.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Maximum number of log entries to return" },
                        "cursor": { "type": "string", "description": "Journald cursor for pagination. Returned in previous log responses." },
                        "before": { "type": "boolean", "description": "If true and cursor is set, return entries before the cursor. Default: false" },
                        "boot": { "type": "string", "description": "Boot identifier to filter logs by (e.g. '0' for current boot, '-1' for previous)" }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.logs",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "server.kernel-logs".into(),
                description: "Fetch recent kernel log entries (dmesg equivalent). Returns kernel \
                    messages with timestamps. Useful for diagnosing hardware issues, driver \
                    problems, or boot errors.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Maximum number of log entries to return" },
                        "cursor": { "type": "string", "description": "Journald cursor for pagination" },
                        "before": { "type": "boolean", "description": "If true, return entries before the cursor. Default: false" },
                        "boot": { "type": "string", "description": "Boot identifier to filter by" }
                    },
                    "additionalProperties": false
                }),
            },
            rpc_method: "server.kernel-logs",
            sync_db: false,
            needs_session: false,
        },
        ToolEntry {
            definition: ToolDefinition {
                name: "package.logs".into(),
                description: "Fetch recent log entries for a specific installed service/package. \
                    Returns the service's stdout/stderr output with timestamps.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Package identifier to get logs for. Read or subscribe to startos:///public/packageData to see installed packages." },
                        "limit": { "type": "integer", "description": "Maximum number of log entries to return" },
                        "cursor": { "type": "string", "description": "Journald cursor for pagination" },
                        "before": { "type": "boolean", "description": "If true, return entries before the cursor. Default: false" },
                        "boot": { "type": "string", "description": "Boot identifier to filter by" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "package.logs",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Shell execution (package containers only — no host-level shell access)
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "package.shell".into(),
                description: "Execute a command inside a package's subcontainer (where the actual \
                    service runs). Returns stdout, stderr, and exit code. Use this to inspect \
                    the service filesystem, check running processes, read config files, or run \
                    service-specific CLI tools. If the package has exactly one subcontainer, it \
                    is selected automatically. If there are multiple, provide 'subcontainer' or \
                    'name' to filter. Commands have a 30-second timeout by default.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Package identifier. Read or subscribe to startos:///public/packageData to see installed packages." },
                        "command": { "type": "string", "description": "Command to execute inside the subcontainer (passed to /bin/sh -c)" },
                        "subcontainer": { "type": "string", "description": "Subcontainer ID filter (partial match). Only needed if the package has multiple subcontainers." },
                        "name": { "type": "string", "description": "Subcontainer name filter (partial match). Alternative to 'subcontainer'." },
                        "timeout": { "type": "integer", "description": "Timeout in seconds. Default: 30. Max: 300." }
                    },
                    "required": ["id", "command"],
                    "additionalProperties": false
                }),
            },
            rpc_method: "__package_shell__",
            sync_db: false,
            needs_session: false,
        },
        // =====================================================================
        // Session management (requires __Auth_session injection)
        // =====================================================================
        ToolEntry {
            definition: ToolDefinition {
                name: "auth.session.list".into(),
                description: "List all active authenticated sessions on this server. Returns \
                    session IDs, last active timestamps, and which session is the current one. \
                    Use this to audit who is connected to the server.".into(),
                input_schema: empty_schema(),
            },
            rpc_method: "auth.session.list",
            sync_db: false,
            needs_session: true,
        },
    ];
    tools
        .into_iter()
        .map(|t| (t.definition.name.clone(), t))
        .collect()
}
