# Subnets

A subnet is an isolated private network (a VLAN) whose devices can all communicate with each other. By default a subnet is a `/24` (254 devices), but the range is configurable: you can give it a larger block (a smaller prefix, e.g. `/16`) for more devices. `/24` is the smallest allowed — StartTunnel rejects a range smaller than `/24`.

StartTunnel comes with a default subnet, which is sufficient for most users. You can create additional subnets to isolate groups of devices from each other.

## Creating a Subnet

1. In StartTunnel, navigate to `Subnets` and click "Add".
1. Name the subnet and accept or customize the **IP range** — the block its devices draw addresses from (a `/24` by default, or a larger block). The range is fixed once the subnet is created; everything else can be changed later by editing the subnet.
1. Optionally set the subnet's [DNS resolver](#dns), [outbound IP](#outbound-ip), and [IPv6 prefix](#ipv6) (all covered below).
1. Click "Save".

## DNS

Each subnet resolves domain names through one of three modes, chosen with the **DNS** field in the Add/Edit dialog:

- **Default (VPS provider)** — use the resolvers your VPS provides. The simplest option, and the default.
- **Device** — designate one device already on the subnet as its resolver (at least one device must exist first). Use this to point the subnet at a resolver you run yourself.
- **Custom** — enter up to three DNS server IP addresses to query directly.

This chooses the _upstream_ resolver the subnet uses. It is separate from the private DNS _records_ StartTunnel serves for your own hostnames — see [DNS Records](/start-tunnel/dns-records.html) for those.

## Outbound IP

If your VPS has more than one public IPv4 address, you can choose which one a subnet's outbound traffic leaves from — its egress (SNAT) address — with the **WAN IP** field:

- **System default** — let StartTunnel choose (masquerade through the server's primary address). When the address it resolves to is known, it's shown in parentheses, e.g. `System default (203.0.113.10)`.
- A specific address — pin egress to one of the VPS's detected public IPv4 addresses.

A single-IP VPS has only one choice, so you can leave this on **System default**. Individual devices can override their subnet's choice — see [Devices › Outbound IP](/start-tunnel/devices.html#outbound-ip).

## IPv6

A subnet can carry a routed IPv6 prefix so every device on it gets a stable, globally-routable address. Set the **IPv6 Prefix** field (e.g. `2001:db8:abcd::/64`) in the Add/Edit dialog, or leave it blank for none. See [IPv6](/start-tunnel/ipv6.html) for prefix sizing, routing, and VPS requirements.

## Removing a Subnet

1. Navigate to `Subnets`, select the subnet, and click "Remove".

> [!WARNING]
> Removing a subnet disconnects all devices on it. Their WireGuard configs will no longer work.
