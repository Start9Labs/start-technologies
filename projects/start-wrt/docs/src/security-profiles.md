# Security Profiles

Security Profiles are the core concept in StartWRT. Every device on the network is assigned a Security Profile that governs what it can access — LAN devices, the Internet, DNS servers, VPN tunnels, and time-of-day restrictions. Profiles replace the need to manually configure VLANs, firewall zones, subnets, and routing tables.

## How Profiles Work

Behind the scenes, each Security Profile creates an isolated network environment:

- **VLAN** — Layer 2 isolation so devices on different profiles cannot see each other's traffic
- **Subnet** — A dedicated `/24` IP range with its own DHCP server and gateway
- **Firewall zone** — Rules controlling what the profile can access (LAN, Internet, specific devices)
- **DNS** — Inherited from the system, the outbound VPN, or overridden with custom servers
- **Outbound routing** — Which gateway or VPN chain handles the profile's Internet traffic
- **WAN Blackout** — Optional time-of-day restrictions on Internet access

You do not need to configure any of these individually. When you create a profile, StartWRT sets up all the underlying networking automatically.

## How Devices Get Profiles

A device's Security Profile is determined by its **point of entry** — how it connects to the network:

- **Ethernet** — The physical port a device plugs into. Each port maps to a profile. See [Ethernet](ethernet.md).
- **Wi-Fi** — The password a device uses to join the Wi-Fi network. Each password maps to a profile. See [Wi-Fi](wifi.md).
- **Inbound VPN** — The WireGuard server a device connects to remotely. Each VPN server maps to a profile. See [Inbound VPNs](inbound-vpn.md).

One SSID, multiple passwords. One router, multiple isolated networks. The profile abstraction keeps it simple.

## Creating a Profile

1. Navigate to `Security Profiles` and click "Add".

1. Enter a **Name** at the top of the dialog (e.g. "Admin", "Guest", "Children", "Smart Devices"). The Name field sits above three tabs — **LAN**, **WAN / Internet**, and **DNS**.

1. On the **LAN** tab, configure local-network settings:

   - **Subnet** — Set the third octet of the profile's `/24` subnet. The first two octets are shown but locked; for example, a value of `2` creates the subnet `192.168.2.0/24`. They are locked because every profile must stay within the primary [LAN network block](lan.md)'s `/16` for cross-subnet routing to work — changing the LAN network block moves all profiles with it. Each profile must have a unique subnet. The gateway address is always `.1` within the subnet (e.g. `192.168.2.1`).

   - **Access control** — Controls which other profiles this profile can communicate with on the local network:
     - **All** — Full access to devices on all profiles.
     - **Same profile** — Only communicate with devices on this same profile.
     - **Whitelist** — Select specific profiles from a list.

   - **Auto whitelist new profiles** — A toggle shown in Whitelist mode. When enabled, newly created profiles are automatically added to this profile's whitelist. Useful for admin profiles that should maintain access to all network segments.

   - **Outbound Routing** — Choose how traffic from this profile reaches the Internet. Select **Direct** for direct Internet access, or **VPN** to route all traffic through an outbound VPN. Choosing VPN reveals an [Outbound VPN](outbound-vpn.md) client picker (disabled if you have no outbound VPN clients).

1. On the **WAN / Internet** tab, configure Internet access:

   - **WAN Access** — Controls Internet access for devices on this profile:
     - **All** — Unrestricted Internet access.
     - **None** — No Internet access. Devices can only reach LAN resources permitted by the LAN access setting.
     - **Whitelist** — Allow connections only to specific destination IPs or CIDR ranges (e.g. `1.1.1.1, 8.8.8.0/24`).
     - **Blacklist** — Block connections to specific destination IPs or CIDR ranges, allow everything else.

   - **WAN Blackout** — An inlined schedule editor for time-of-day Internet restrictions (see [WAN Blackout](#wan-blackout) below). It is disabled when WAN Access is None; any existing windows are retained.

1. On the **DNS** tab, choose **Inherit from system** or **Custom**. Custom lets you specify up to three DNS servers, each with an optional **DoH** (DNS-over-HTTPS) toggle. When inheriting, the profile uses the outbound VPN's DNS (if routing through a VPN) or the system DNS from [WAN Settings](wan.md).

1. Click "Save".

## Editing a Profile

1. Navigate to `Security Profiles` and select the profile.

1. Modify any settings and click "Save".

> [!WARNING]
> Changing a profile's settings takes effect immediately for all devices currently assigned to that profile.

## Deleting a Profile

1. Navigate to `Security Profiles` and select the profile.

1. Click "Delete".

> [!WARNING]
> Deleting a profile disconnects all devices assigned to it. Associated points of entry (Wi-Fi passwords, Ethernet port assignments, VPN servers) are automatically removed.

> [!NOTE]
> The primary LAN profile cannot be deleted.

## WAN Blackout

Each profile can optionally restrict Internet access during specific time periods. WAN Blackout defines **block windows** — periods when WAN access is removed for devices on the profile. Wi-Fi connectivity and LAN access are unaffected. Outside of these windows, the profile's normal WAN access rules apply.

WAN Blackout is edited inline on the **WAN / Internet** tab of the profile create/edit dialog (see [Creating a Profile](#creating-a-profile)).

1. The schedule is displayed as a 7-day visual timeline grid, with one row per day of the week.

1. Click "Add" to create a block window:

   - Set the **start** and **end** times. Times use a 12-hour `HH:MM AM/PM` format, with a 15-minute quick-pick dropdown. A window may cross midnight (e.g. 10:00 PM to 6:00 AM). Setting the start time equal to the end time creates a full 24-hour window.
   - Select which **days** of the week the window applies to.
   - Click "Save".

1. Multiple block windows per day are supported.

Overlapping windows are rejected when you save. A schedule that covers the entire week with no gap is also rejected — the system needs at least one boundary to toggle WAN access on and off.

> [!TIP]
> Click a window once to edit it. Removing a window does not ask for confirmation.

> [!NOTE]
> WAN Blackout blocks Internet access, not LAN access. Devices can still reach LAN resources during blocked periods according to the profile's LAN access setting. For disabling the Wi-Fi radio itself on a schedule (affecting all Wi-Fi devices), use [Wi-Fi Blackout](wifi-schedules.md).

## Example Profiles

Here is an example of how a household might use Security Profiles:

| Profile | WAN Access | LAN Access | DNS | Outbound Routing | WAN Blackout |
|---------|------------|------------|-----|-------------------|----------|
| **Admin** | All | All | Inherit | Mullvad VPN | — |
| **Children** | All | Same profile | Custom (filtering) | DNS-filtering VPN | Block 9 PM - 7 AM |
| **Guest** | All | Same profile | Inherit | Proton VPN | — |
| **Smart Devices** | Whitelist | Same profile | Inherit | Direct | — |
| **Shared Services** | None | Whitelist | Inherit | Direct | — |
