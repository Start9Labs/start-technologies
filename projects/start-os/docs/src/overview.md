# Overview

StartOS is a Linux distribution built for one purpose: running a personal server. It runs self-hosted software as services, and you administer it from your browser, or from the command line with start-cli.

## Services

A service is a piece of software, usually an open-source project developed elsewhere (its upstream), wrapped in a package that teaches StartOS how to run it. The package turns the software's configuration files into validated forms and its commands into buttons, declares its health checks and its dependencies on other services, chooses what to back up, and offers every way to reach it.

You install packages from the [Marketplace](marketplace.md), a network of registries. StartOS comes with two: the Start9 Registry, which carries the packages Start9 publishes, and the Community Registry, which carries packages other developers maintain. Anyone can run another registry, and you can also [sideload](sideloading.md) a package from a file. **System → Registries** manages the registries you browse and which one provides StartOS updates; [Trusted Keys](trusted-keys.md) independently controls which signing keys you accept for packages and StartOS updates. A registry's keys are trusted for packages when you add it; keys it offers later start untrusted, each with a notification that offers to trust it.

Registry downloads prefer BitTorrent when available, with HTTP fallback. HTTP servers already see an IP address; BitTorrent widens that audience to trackers and peers. [Software sharing](marketplace.md#software-sharing) shows the StartOS default gateway and controls downloads, optional sharing of installed S9PKs and stored OS images, and incoming connections. Sharing keeps normal file retention lifetimes.

Each service runs in its own isolated container. StartOS doesn't use Docker or Podman, so instructions written for them don't apply. A service's package sets its software's configuration, and can reset a setting that was changed by hand inside the container. Where a package gives you a way to change something, use it: instructions for installing and configuring the software elsewhere may not apply on StartOS.

On a service's page, its package gives you:

- [Actions](actions.md): buttons, some of which open a form, that do what an administrator would otherwise do on the command line, such as showing credentials, changing a setting, resetting a password or connecting an integration. Some are available only while the service is running, or only while it's stopped.
- [Tasks](tasks.md): notifications that guide you through setup and upkeep, most of them by pointing to an action on this service or on a service it depends on. Each task is Required, Important or Recommended; while a Required task is open, the service stays stopped.
- [Interfaces](interfaces.md): the service's web UIs, APIs and peer-to-peer endpoints.
- [Instructions](instructions.md): the package developer's guide to setting up and using the service on StartOS.
- [Health checks](health-checks.md) and [logs](logs.md).

A package's README, in its repository, is the developer's account of how the package is built and how it differs from its upstream. Together, a service's Instructions and README cover setting it up and configuring it on StartOS; the upstream project's documentation covers the software itself.

## Reaching your services

Each interface lists every address it can be reached at: its LAN and IPv6 addresses, your server's .local name, your public IP address, and your domains. Each address turns on or off on its own, and public addresses stay off until you turn them on. A public domain gets a Let's Encrypt certificate. To connect over HTTPS on your local network or over a VPN, a device must [trust your server's Root CA](trust-ca.md), the certificate authority your server creates during setup; devices that connect over Tor or a public domain don't need it. Installing the Tor service gives any interface an onion address.

A [gateway](gateways.md) carries your server's traffic. Your home router and a StartTunnel server are inbound and outbound gateways, and a VPN provider's WireGuard configuration is an outbound-only gateway. When you make an address public, StartOS asks the gateway to open the port, and closes it again when you turn the address off. StartTunnel does this on its own, and StartWRT does once you allow it for your server.

## Backups

StartOS [backs up](backup-create.md) the whole server, or only the services you choose, to a drive plugged into the server or to a shared folder on another computer on your network. Backups are encrypted with your master password; the server's own data drive is not encrypted with it, so anyone holding the server or its drive can read your services' data. Each new backup to a target updates the one already there. Keep backups on multiple targets to protect against losing a drive or network share; advanced users can also retain [read-only snapshots](backup-create.md#read-only-snapshots) on one target. You can [restore](backup-restore.md) a single service, or the whole server, including onto new hardware.

## The command line

For what no action covers, you can [SSH](ssh.md) into your server and run commands inside a service's container with `start-cli package attach`. The package's README names its subcontainers. Changing files there can break the service or lose data; see [Accessing Service Containers](service-containers.md).
