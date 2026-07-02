# Ethernet

Each physical Ethernet port on the StartWRT router maps to a [Security Profile](security-profiles.md). The port a device plugs into determines its profile — no manual configuration is needed on the device itself.

## How It Works

When a device is plugged into an Ethernet port, the router assigns it to the profile mapped to that port. The device receives an IP address on that profile's subnet and is subject to that profile's firewall rules, VPN routing, and schedule restrictions.

Behind the scenes, StartWRT uses bridge VLAN filtering to isolate traffic between ports. Each port tags traffic with a different VLAN ID, ensuring devices on different profiles cannot communicate at Layer 2 unless explicitly allowed.

## WAN Port

One Ethernet port is designated as the WAN port, which connects to your ISP modem or upstream network. The WAN port is shown with a "WAN" badge in the port list and cannot be assigned a Security Profile.

## Assigning Profiles to Ports

1. Navigate to `Points of Entry > Ethernet`.

1. For each non-WAN port, select the desired Security Profile from the dropdown.

1. Click "Save".

> [!NOTE]
> Changes take effect immediately. A device currently plugged into a port will be reassigned to the new profile without needing to unplug and replug.

> [!NOTE]
> If you connect a network switch to an Ethernet port, all devices on that switch share the same Security Profile. StartWRT cannot differentiate between devices beyond its onboard Ethernet ports. Those devices still appear individually in the [Devices](devices.md) list — including hosts with static IPs or only IPv6 addresses — because the router learns them from the bridge forwarding table even without a DHCP lease.

## Changing the WAN Port

If you need to use a different physical port for your ISP connection:

1. Navigate to `Points of Entry > Ethernet`.

1. Click "Change WAN Port".

1. Select the new port.

1. Confirm the change.

> [!WARNING]
> Changing the WAN port restarts the network and may briefly interrupt your Internet connection. Ensure your modem cable is connected to the new port before confirming.

## Example

| Port | Profile | Use Case |
|------|---------|----------|
| Port 1 | *(WAN)* | Connected to ISP modem |
| Port 2 | Admin | Desktop computer with full LAN and Internet access |
| Port 3 | Admin | StartOS server with full LAN and Internet access |
| Port 4 | Guest | Guest-accessible Ethernet jack in the living room |
