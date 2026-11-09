# Ethernet

Each physical Ethernet port on the StartWRT router, and each USB port you plug a USB Ethernet adapter into, maps to a [Security Profile](security-profiles.md). The port a device plugs into determines its profile — no manual configuration is needed on the device itself.

## How It Works

When a device is plugged into an Ethernet port, the router assigns it to the profile mapped to that port. The device receives an IP address on that profile's subnet and is subject to that profile's firewall rules, VPN routing, and schedule restrictions.

Behind the scenes, StartWRT uses bridge VLAN filtering to isolate traffic between ports. Each profile has its own VLAN, and each port belongs to the VLAN of the profile mapped to it, so devices on different profiles cannot communicate at Layer 2. Traffic between profiles passes through the router's firewall, as each profile's LAN access allows.

## WAN Port

One Ethernet port is designated as the WAN port, which connects to your ISP modem or upstream network. The WAN port is shown with a "WAN" badge in the port list and cannot be assigned a Security Profile.

## Assigning Profiles to Ports

1. Navigate to `Points of Entry > Ethernet`.

1. For each non-WAN port, select the desired Security Profile from the dropdown.

1. Confirm the change. The confirmation names any devices on the port that have a reserved address or [Published Ports](published-ports.md) rule for the port's old profile. Those are kept, and apply again if the devices return to that profile. Canceling leaves the port unchanged.

> [!NOTE]
> Changes take effect as soon as you confirm. The router briefly disconnects the port, and most devices plugged into it rejoin on the new profile's subnet within seconds. A device behind a switch, or one that ignores the brief disconnect, keeps its old address and usually loses Internet access and its connections to other devices until it reconnects. Unplug and replug its cable, or restart the device or the switch. Otherwise it reconnects on its own within 12 hours.

> [!NOTE]
> If you connect a network switch to an Ethernet port, all devices on that switch share the same Security Profile. StartWRT cannot differentiate between devices beyond its own ports. Those devices still appear individually in the [Devices](devices.md) list — including hosts with static IPs or only IPv6 addresses — because the router learns them from the bridge forwarding table even without a DHCP lease.

## USB Ethernet Adapters

A USB Ethernet adapter in one of the router's USB-A ports adds a LAN port. The port appears in the port list as `usb_eth0`, `usb_eth1`, and so on, the first time a network device is plugged into it, in the Admin profile. Assign it a profile like any Ethernet port.

The profile belongs to the router's USB port, not to the adapter:

- A different adapter in the same USB port gets the same profile.
- The same adapter in a different USB port gets that port's profile.
- An unplugged USB port stays in the list, marked "Not connected", and keeps its profile.
- Each port of a USB hub is a USB port of its own, with its own profile. Moving the hub to another USB port moves its adapters to new USB ports.

To remove a USB port from the list, unplug its adapter and click the trash icon in its row. While an adapter is plugged in, the router asks you to unplug it first. The network restarts briefly. The port appears again, in the Admin profile, the next time an adapter is plugged into it.

Adapters with Realtek (RTL8152, RTL8153, RTL8156) or ASIX (AX88179, AX88772) chips work, as do adapters that use the standard USB Ethernet classes (CDC ECM, CDC NCM, RNDIS). The router's USB-C port supplies its power and does not take an adapter. An adapter with a USB-C plug needs a USB-C to USB-A adapter. The WAN port must be an Ethernet port.

## Changing the WAN Port

If you need to use a different physical port for your ISP connection:

1. Navigate to `Points of Entry > Ethernet`.

1. Click "Change WAN Port".

1. Select the new port.

1. Confirm the change.

> [!WARNING]
> Changing the WAN port restarts the network and may briefly interrupt your Internet connection. Ensure your modem cable is connected to the new port before confirming.

## Example

| Port | Profile | Use Case                                                                                                |
| ---- | ------- | ------------------------------------------------------------------------------------------------------- |
| eth1 | _(WAN)_ | Connected to ISP modem                                                                                  |
| eth0 | Admin   | The included switch, with a desktop computer and a StartOS server that get full LAN and Internet access |
