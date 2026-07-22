# DNS Records

StartTunnel runs a private DNS resolver for your tunnel. It lets you reach the services you host on tunnel devices by a memorable hostname — e.g. `home.example.com` or `git.lan` — instead of memorizing each device's tunnel IP. These names are **private to your tunnel**: they resolve for connected devices only, and are never published to the public Internet.

## Why use it

- **Reach services by name.** Point `git.example.com` at your server once; every connected device can use it, and it keeps working even if you rebuild the service.
- **One source of truth.** Everyone on the tunnel resolves the same names to the same device — no per-device `hosts` files to maintain.
- **Private.** The records exist only inside your tunnel; the outside world can't see or resolve them.

## How it works

Devices connected to the tunnel use StartTunnel as their DNS resolver. When a device looks up one of your private hostnames, StartTunnel answers from the records on the `DNS Records` page; any other lookup falls through to the subnet's configured upstream resolver (see [Subnets › DNS](/start-tunnel/subnets.html#dns)). A record is one of:

- **A** / **AAAA** — a hostname → a device's tunnel IPv4 / IPv6 address.
- **CNAME** — a hostname → another hostname (an alias).
- **TXT** — a hostname → arbitrary text (e.g. a verification string).

Records come from two places: you add them **by hand**, or a trusted device **injects** them automatically over [RFC 2136](https://www.rfc-editor.org/rfc/rfc2136).

> [!NOTE]
> A StartOS server using this tunnel injects the records for its private domains — and for its own `.local` name — **automatically**, as long as its device is a Server with DNS injection enabled (which it is by default — see below). You normally won't need to add records by hand.

## Allowing a device to inject records

DNS injection is a **Server** capability: a device added as a Server has it **on by default** (along with auto-publish), and Clients don't have it at all. Leave it enabled only for devices you control and trust — turn it off from the Servers table for any server that shouldn't manage your DNS.

> [!WARNING]
> A device allowed to inject DNS records can create, overwrite, or delete any record StartTunnel serves. Keep this enabled only for trusted devices, such as your own StartOS server.

1. In StartTunnel, navigate to `Devices`. DNS injection is a **Server** capability — if the device is a Client, change it to a Server first (see [Devices](/start-tunnel/devices.html)). A newly-added Server (or a Client promoted to Server) already has DNS injection on.

1. In the Servers table, the **DNS injection** toggle controls the capability — leave it on to allow injection, or turn it off to withhold it.

An enabled device may add, update, and remove records via RFC 2136 DNS UPDATE. StartTunnel authorizes each request by the device's tunnel IP — IPv4 or IPv6 — so only that device's allowance is in effect.

## Viewing and managing records

1. In StartTunnel, navigate to `DNS Records`.

1. Records are shown in two tables: **Manual** (records you added by hand) and **Automatic** (records injected by a device, each showing the injecting device's IP as its source).

1. To add a record manually, click "Add" on the Manual table, enter the name, type (A, AAAA, CNAME, or TXT), value, and TTL, and click "Save".

1. To remove a record, select it and click "Remove".
