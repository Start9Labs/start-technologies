# Devices

Every device on a StartTunnel subnet gets its own WireGuard configuration. Devices come in two kinds, listed in separate tables on the `Devices` page:

- **Servers** — a StartOS server that may configure the gateway on its own behalf: injecting DNS records for its private domains and auto-publishing ports (via PCP/UPnP). Both capabilities are on by default for a Server.
- **Clients** — a plain peer such as a phone or laptop that uses the tunnel for connectivity only, with no gateway-configuration abilities.

## Adding a Device

1. In StartTunnel, navigate to `Devices` and click **Add** — on the **Servers** table for a StartOS box, or on the **Clients** table for a phone or laptop. Which button you click sets the device's kind (there is no kind selector in the form).

1. Fill out the form:
   - Give the device a name (e.g. "Start9 Server", "Phone", "Laptop").
   - Select a subnet (the default is fine).
   - Accept or choose an IP address on the subnet (the default is fine).
   - Optionally set the [outbound IP](#outbound-ip) — the WAN address the device's traffic leaves from.
   - For a Server, **Allow DNS injection** and **Allow auto-publish** are enabled by default; uncheck either to withhold that capability. A Client has neither.
   - Click "Save".

1. Download the resulting `start-tunnel.conf` (or copy to your clipboard).

1. Import the config into the appropriate app on the device:
   - **StartOS server**: Navigate to `System > Gateways`, click "Add", name the gateway (e.g. "StartTunnel"), upload or paste the config, and click "Save". StartOS will now see the VPS as a gateway, and each service interface will automatically acquire new addresses corresponding to it.
   - **Phone or tablet**: Scan the QR code shown in StartTunnel using the [WireGuard app](https://www.wireguard.com/install/).
   - **Laptop or desktop**: Download the config and import it into the [WireGuard app](https://www.wireguard.com/install/).

## Server capabilities

A Server has two independently-toggleable capabilities, shown as switches in the Servers table:

- **DNS injection** — lets the server manage the DNS records StartTunnel serves for your private domains (see [DNS Records](/start-tunnel/dns-records.html)).
- **Auto-publish** — lets the server publish its own ports via PCP/UPnP (see [Published Ports](/start-tunnel/published-ports.html)).

Only enable these for servers you trust. Clients have neither capability.

## Changing a device's role

Use a device's actions menu to **Change to Server** or **Change to Client**. Changing to Server turns both Server capabilities on; changing to Client turns them off and moves the device to the Clients table.

## Outbound IP

By default a device's outbound traffic leaves from its subnet's [outbound IP](/start-tunnel/subnets.html#outbound-ip). On a VPS with more than one public IPv4 address, you can override this per device with the **WAN IP** field in the device's Add/Edit dialog — choose **Subnet default** to inherit the subnet's setting (the address it resolves to is shown in parentheses), or a specific address to pin this device's egress. On a single-IP VPS there is only one choice, so leave it on **Subnet default**.

## Removing a Device

1. Navigate to `Devices`, select the device, and click "Remove".
