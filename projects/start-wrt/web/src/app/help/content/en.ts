// prettier-ignore
/** English help content (source of truth, eager). Route -> markdown. */
export const HELP_EN: Record<string, string> = {
  '/devices': `## Devices

View and manage all devices on your network. Online devices are currently connected, offline devices have connected before but aren't active now.

### Name

The device's hostname or a custom name you've assigned. Click to view details and configure the device.

### Connection

How the device connects to your network (Ethernet, Wi-Fi).

### MAC Address

The unique hardware identifier for the device's network interface.

### IP Address

The device's assigned IPv4 and IPv6 addresses.

### Data & Speed

Bandwidth usage and current transfer speeds. Useful for identifying heavy network users.`,
  '/devices/device': `## Devices

View device information and configure settings.

### Summary

Displays the device's current status, connection type, security profile, and IP addresses. A lock icon indicates a reserved IP address.

### Data Usage

Shows historical network usage for this device. Use the dropdown to view different time periods. Download is shown in blue, upload in green.

### Name

Assign a custom name to easily identify this device. If left empty, the device's hostname will be used. The placeholder shows what the name will default to.

### Reserved IP

Reserve an IP address to assign a fixed IPv4 address that won't change between reboots. Useful for servers, printers, NAS devices, or anything you need to access by a consistent IP address.

### Forget

Removes an offline device from your device list. Any custom name or reserved IP settings will be lost. If the device reconnects, it will appear as a new device.`,
  '/ethernet': `## Ethernet

### Port

The physical Ethernet port identifier (e.g., eth0, eth1).

### Security Profile

The default security profile assigned to devices connected via the specified Ethernet port.

### WAN

The Wide Area Network (WAN) port connects to your ISP modem or upstream network. Only one Ethernet port can be designated as WAN at a time.

### Change WAN Port

Use the "Change WAN Port" button to reassign the WAN designation to a different Ethernet port. This will interrupt your internet connection and restart the router. Ensure your modem is connected to the new WAN port before confirming.`,
  '/ethernet/dialog': `## Change WAN Port

Select a new Ethernet port to serve as the WAN (internet) connection. The current WAN port is preselected.

Changing the WAN port will immediately interrupt your internet connection and restart the router. Ensure your modem or upstream network cable is connected to the new port before confirming.`,
  '/inbound': `## Inbound VPNs

VPN servers are software applications that provide secure and encrypted access to internal network resources, allowing clients to create an inbound connection to the network securely over the internet from anywhere in the world.

### Security Profile

Displays the access permissions set for the device. Manages and controls what the device can access on the network via a security profile or schedule.

### Port

The network port on which the VPN server listens for incoming connections.`,
  '/inbound/dialog': `## Inbound VPNs

### Label

A clear and descriptive name for the VPN server, such as "Office VPN Server" or "Home VPN Server," to differentiate it from other VPN connections.

### Endpoint

The IP address or domain name where the VPN server can be reached by clients connecting over the internet.

### Security Profile

The initial set of access rights and restrictions assigned to clients when they connect to the VPN server. Select a previously created profile or schedule, or create a new one.

### Port

The network port on which the VPN server listens for incoming connections. Common port used for WireGuard is 51820.`,
  '/inbound/client': `## Inbound VPNs

Manage client devices for this VPN Server.

### Name

A chosen name to easily identify the purpose of the client device.

### LAN IP Address

The assigned IP address on the local area network. This was configured when the client device was setup.

### Routing

Shows the client's traffic routing mode. "All traffic" routes all internet traffic through the VPN tunnel. "LAN only" routes only local network traffic through the tunnel. Use the actions menu to switch between modes.`,
  '/inbound/client/dialog-add': `## Inbound VPNs

### Label

A clear and descriptive name for the client device, such as "John's Laptop" or "Office Phone," to differentiate it from other devices.

### LAN IP Address

The desired LAN IP address for the client device, ensuring it is within the IP range allocated for the VPN. Ensures the client device has a unique IP address within the VPN network, allowing it to communicate with other devices.

### Public Key

The WireGuard public key for the client device. If left empty, a key pair will be generated automatically. Enter an existing public key if the client device already has one configured.

### Route all traffic through tunnel

When enabled, all of the client's internet traffic is routed through the VPN tunnel. When disabled (the default), only traffic destined for the local network passes through the tunnel and the client uses its own internet connection for everything else.`,
  '/inbound/client/dialog-config': `## Client Configuration

The WireGuard configuration for this client device. Use this to set up the VPN connection on the client.

### File

Displays the configuration as text. Use the copy button to copy it to your clipboard, or the download button to save it as a <code>wireguard.conf</code> file that can be imported directly into the WireGuard app.

### QR

Displays the configuration as a QR code. Scan it with the WireGuard mobile app to configure the client device without manual entry.`,
  '/inbound/client/dialog-rename': `## Rename Client

### Name

Enter a new name for this VPN client device. Use something descriptive, such as "John's Laptop" or "Office Phone," to easily identify the device in your client list.`,
  '/lan/ipv4': `## LAN – IPv4

### Network Block

The /16 private IP block for your network. Each security profile will receive its own /24 subnet within this block, allowing up to 256 separate subnets with 254 devices each.

### Router IP

The router's LAN IP is the gateway (.1) of the Admin security profile's subnet. The first two octets come from the Network Block above; the third octet is set by the Admin profile's Subnet field, not on this page.`,
  '/lan/ipv6': `## LAN – IPv6

### IPv6

When enabled, devices on your network will receive IPv6 addresses via SLAAC (Stateless Address Autoconfiguration). This is the standard method for IPv6 address assignment.

IPv6 allows your devices to be directly reachable from the internet without NAT, which is useful for hosting services or peer-to-peer applications.

### Prefix Length

The prefix length determines the size of your LAN's IPv6 address space. It must be **larger** (a higher number) than your WAN prefix to create a valid subnet.

For example, if your ISP assigns you a /48 prefix, you can use /56, /60, or /64 for your LAN. A /64 prefix is recommended for most home networks.`,
  '/outbound': `## Outbound VPNs

Outbound VPNs route your internet traffic through encrypted tunnels to remote servers, enhancing privacy and security.

### Why use outbound VPNs?

-   Hide your IP address from websites and services
-   Encrypt traffic on untrusted networks
-   Access geo-restricted content
-   Prevent ISP tracking

### VPN Chaining

Route one VPN through another for additional privacy. Your traffic exits through multiple servers, making it harder to trace back to you. Note that chaining increases latency.

### Getting Started

Upload a WireGuard configuration file from your VPN provider (Mullvad, ProtonVPN, IVPN, etc.) to create a new VPN client.`,
  '/outbound/vpn': `## Outbound VPNs

<details><summary>Label</summary> A friendly name to identify this VPN connection. Use something descriptive like "Mullvad Sweden" or "Work VPN". </details> <details><summary>Connects to</summary> Where traffic from this VPN should be routed:<ul><li><b>Internet:</b> Traffic exits directly to the internet through this VPN.</li><li><b>Another VPN:</b> Chain through another VPN first for extra privacy. Your traffic will be encrypted multiple times and exit through multiple servers.</li></ul><b>Note:</b> Chaining VPNs increases latency and may reduce speeds. </details> <details><summary>Connection Path</summary> Shows the full route your traffic takes from this VPN to the internet. For example: Mullvad → Proton → Internet means traffic is encrypted by Mullvad, sent through Proton, then exits to the internet.</details>`,
  '/outbound/dialog': `## Outbound VPNs

VPN clients create an outbound connection to a VPN server, allowing devices to send and receive data as if they were directly connected to the private network. Masks your IP address, bypasses geographic restrictions, and protects sensitive data on public Wi-Fi networks.

### Label

A clear and descriptive name for the VPN connection, such as "Mullvad Sweden" or "Work VPN," to easily differentiate it from other connections.

### Config File

Upload a WireGuard **.conf** file from your VPN provider. This file contains the server address, keys, and connection settings needed to establish the tunnel. Most providers (Mullvad, ProtonVPN, IVPN, etc.) offer WireGuard config file downloads from their website or app.

### Target

The next hop in the path of a layered VPN connection.`,
  '/profiles': `## Profiles

Security profiles define network segments with isolated permissions. Each profile controls internet access and communication with other profiles on your local network.

### Name

A descriptive name for the security profile, such as "Guest" or "IoT Devices".

### DNS

Which DNS servers devices in this profile use — the system's ("System"), the profile's own custom servers ("Custom"), or, when routing through a VPN, the VPN's ("VPN").

### Outbound

How traffic from this profile reaches the internet — directly ("Direct"), or through one of your VPN clients (shown by name).

### LAN Access

Which other profiles this profile can communicate with on the local network — all of them, only its own, or a chosen whitelist.

### WAN Access

Whether devices in this profile have internet access, optionally restricted by a whitelist or blacklist of IP addresses or CIDR ranges.`,
  '/profiles/dialog': `## Profiles

Security profiles define network segments with isolated permissions. Each profile controls internet access and communication with other profiles on your local network.

### Name

A descriptive name for the security profile, such as "Guest" or "IoT Devices".

### Subnet

The third octet of the IPv4 subnet for this profile. For example, a value of 2 creates the subnet 192.168.2.0/24. Each profile must have a unique subnet.

### LAN Access

Controls which other profiles this profile can communicate with on the local network. Choose "All" for full access, "Same profile" for isolation, or "Whitelist" to select individual profiles.

### Auto whitelist new profiles

When LAN access is set to "Whitelist", enabling this also grants access to any profiles created in the future — useful for admin profiles that should reach all network segments.

### Outbound Routing

Choose how traffic from this profile reaches the internet. Select "Direct" for normal internet access, or "VPN" to route all of the profile's traffic through one of your VPN clients.

### WAN Access

Whether devices in this profile have internet access. Use "Whitelist" or "Blacklist" to restrict access to specific IP addresses or CIDR ranges, or "None" to block the internet entirely.

### Blackout Schedule

Designated times when WAN (internet) access is blocked for devices on this profile. Outside of these windows the profile's normal WAN access rules apply. For example, a "Kids" profile might block WAN access at 7 PM on school nights and 9 PM on weekends, while an "Admin" profile retains full 24/7 access.

This section is disabled when WAN access is set to "None" — the internet is already blocked at all times, so there is nothing to schedule. Any windows you've configured are kept and take effect again if you re-enable WAN access.

### DNS

Choose whether this profile uses the system's DNS servers or its own. Select "Custom" to specify up to three DNS servers, each with optional DoH (DNS over HTTPS) encryption for secure lookups.`,
  '/profiles/blackout': `## Profiles – Blackout times

A blackout window blocks WAN (internet) access for all devices on this profile during the specified time period. The profile's normal WAN access rules apply outside of blackout windows.

### Time Window

The start and end times for the blackout period. WAN access will be blocked between these times on the selected days. Set an end time earlier than the start to wrap the window past midnight (e.g. 22:00–06:00), or set the end equal to the start to block the full 24 hours.

### Days

Select which days of the week the blackout window should be active.`,
  '/profiles/schedule': `### Blackout Schedule

Designated times when WAN (internet) access is blocked for devices on this profile. Outside of these windows the profile's normal WAN access rules apply. For example, a "Kids" profile might block WAN access at 7 PM on school nights and 9 PM on weekends, while an "Admin" profile retains full 24/7 access.

Hover/tap a day and click + to add a window, click an existing window to edit or delete it.`,
  '/published-ports': `## Published Ports

### Published Ports

Make selected ports on a device reachable from the internet. Traffic from the internet on the specified port will be forwarded to the selected device.

### Label

A descriptive name to help you identify this rule, such as "Home Assistant" or "Minecraft Server".

### Device

The device you select will be assigned a stable IPv4 address to ensure the port forwarding rule always reaches the intended device. IPv6 addresses are typically stable through SLAAC.

### Port

The port or port range on the device to expose. For example, "443" for a single port or "27015-27030" for a range.

### Protocol

Choose TCP for most services (web, SSH, etc.), UDP for gaming or VoIP, or both if the service requires it.

### Source

Limit who can connect by specifying an IP address or CIDR range. Use "Any" to allow connections from anywhere on the internet.

### Endpoints

The public addresses where this port can be reached. IPv4 endpoints use NAT to forward traffic, while IPv6 opens the firewall directly to the device.`,
  '/published-ports/dialog': `## Published Ports

Make selected ports on a device reachable from the internet. Traffic from the internet on the specified port will be forwarded to the selected device.

### Label

A descriptive name to help you identify this rule, such as "Home Assistant" or "Minecraft Server".

### Device

The device to forward traffic to. The selected device will be assigned a stable IPv4 address to ensure the rule always reaches the intended device.

### Port

The port or port range on the device to expose. For example, "443" for a single port or "27015-27030" for a range.

### Protocol

Choose TCP for most services (web, SSH, etc.), UDP for gaming or VoIP, or both if the service requires it.

### Source

Limit who can connect by specifying an IP address or CIDR range. Use "Any" to allow connections from anywhere on the internet.

### IP Version

Choose which IP versions to publish on. IPv4 uses NAT to forward traffic, while IPv6 opens the firewall directly to the device.

### External Port

The port number visible to the outside world. Use "Same as device" to keep it identical, or specify a different port for external access.`,
  '/wan/ipv4': `## WAN – IPv4

### IP Address

-   **DHCP:** Your ISP automatically assigns an IP address to your router. This is the most common configuration.
-   **PPPoE:** Used by some DSL providers. Requires a username and password from your ISP.
-   **Static:** Manually configure a fixed IP address. Only use if your ISP has assigned you a static IP.`,
  '/wan/ipv6': `## WAN – IPv6

### IP Address

-   **SLAAC:** Automatic IPv6 configuration. The most common option if your ISP supports IPv6.
-   **DHCPv6:** ISP assigns IPv6 address via DHCP. Use if SLAAC doesn't work with your ISP.
-   **Static:** Manually configure a fixed IPv6 address assigned by your ISP.
-   **6RD:** Tunnels IPv6 over an IPv4 connection. Requires configuration details from your ISP.
-   **Disabled:** Disables IPv6 on the WAN interface.

### IPv6 Prefix

For SLAAC and DHCPv6, the prefix specifies the requested prefix length for prefix delegation (e.g., /48, /56, /64). Leave empty to let your ISP decide automatically.

### DNS

DNS (Domain Name System) translates domain names to IP addresses.

-   **Get from ISP:** Use DNS servers provided automatically by your ISP.
-   **Custom:** Specify your own DNS servers. Both IPv4 and IPv6 addresses are supported.`,
  '/wan/mac-address': `## WAN – MAC Address

### MAC Address

The MAC address identifies your router to your ISP. Some ISPs lock service to a specific MAC address.

-   **Router:** Use the router's built-in MAC address.
-   **Custom:** Clone the MAC address of a previous device. Useful if your ISP has locked service to your old router or modem's MAC address.`,
  '/wan/dns': `## WAN – DNS

### DNS

DNS (Domain Name System) translates domain names to IP addresses.

-   **Get from ISP:** Use DNS servers provided automatically by your ISP.
-   **Custom:** Specify your own DNS servers (e.g., 1.1.1.1, 8.8.8.8).

### Secure (DoH)

Enable DNS over HTTPS (DoH) for encrypted DNS queries. This improves privacy by preventing eavesdropping on your DNS traffic.

**Note:** Not all DNS servers support DoH. Common servers that do include Cloudflare (1.1.1.1), Google (8.8.8.8), and Quad9 (9.9.9.9).`,
  '/wan/dynamic-dns': `## WAN – Dynamic DNS

### Dynamic DNS

DDNS maps your changing IP address to a fixed domain name, allowing you to access your network remotely without knowing your current IP.

-   **Start9:** Free DDNS service provided by Start9. No additional setup required.
-   **Other providers:** Requires an account with the provider. Enter the credentials and hostname from your DDNS provider.`,
  '/settings/activity': `## Settings – Activity

View recent login attempts and security events. Monitor for suspicious activity and remove old entries as needed.`,
  '/settings/advanced': `## Settings – Logs

Advanced options for power users. LuCI provides direct access to OpenWRT configuration. Support diagnostics help troubleshoot issues. Factory reset restores default settings.`,
  '/settings/backup': `## Settings – Backup

### Create Backup

Download a backup of all router settings, including security profiles, WiFi configuration, SSL certificates, and system preferences. Store the backup file in a safe location.

### Restore Backup

Upload a previously created backup file to restore all settings. The router will reboot after restoring and all current settings will be overwritten.`,
  '/settings/general': `## Settings – General

<details><summary>Preferences</summary> General preferences for the router's user interface and behavior.<h3>Theme</h3>Choose from dark or light theme, or keep your system settings applied to the UI.<h3>Language</h3>Configure the preferred language for the user interface of the router. This setting affects the language displayed for all menus, options, and other text elements within the router's web interface.<h3>Timezone</h3>Set the local timezone for the router. This affects scheduled events such as WAN blackout windows and WiFi blackout schedules. </details> <details><summary>Remote Access</summary> Enables remote access to manage the router's settings and configurations from outside the local network.<ul><li><b>When behind NAT</b> (Network Address Translation): Allows remote access only when the router is part of a private network using NAT. NAT is commonly used in home and small business networks where multiple devices share a single public IP address.</li><li><b>Never:</b> Completely disables remote access. Suitable for environments where security is paramount, such as in highly sensitive or isolated networks.</li><li><b>Always:</b> Enables remote access at all times. Ideal for environments where continuous remote management is necessary, such as in a business network where IT administrators need to maintain constant access to the router for monitoring and updates.</li></ul></details><details><summary>Security</summary> Download your Root CA certificate to trust HTTPS connections from additional devices. Installing this certificate allows browsers and apps to verify the router's identity without security warnings. </details> <details><summary>Updates</summary> When an update is available, a banner will appear at the top of this page. You can view release notes before updating. The router will experience brief downtime during the update process.</details>`,
  '/settings/logs': `## Settings – Logs

Logs record system events and help diagnose issues. Useful for troubleshooting and support inquiries.`,
  '/settings/password': `## Settings – Password

Changes the administrator password for the router. Enhances security by protecting router settings, preventing unauthorized access. Set a strong, unique password.`,
  '/settings/ssh-keys': `## Settings – SSH Keys

SSH keys allow secure, passwordless authentication to your router. Add public keys for users who need SSH access.`,
  '/settings/ssh-keys/dialog': `## Settings – SSH Keys

SSH keys allow secure, passwordless authentication to your router. Add the public key for a user who needs SSH access.

### Public Key

Paste the full public key string, including the algorithm prefix and key data. Supported formats include ssh-ed25519, ssh-rsa, and ecdsa-sha2-nistp256/384/521.`,
  '/wifi/blackout-schedule': `## Wi-Fi – Blackout Schedule

Specifies designated times when WiFi will be disabled, preventing any devices from connecting to it. This feature is often used to restrict internet access during specific periods for various purposes, such as reducing distractions, enforcing bedtime routines, or enhancing network security during off-hours.

Hover/tap a day and click + to add a window, click an existing window to edit or delete it.`,
  '/wifi/blackout-schedule/dialog': `## Wi-Fi – Blackout Schedule

A blackout window disables WiFi during the specified time period, preventing any devices from connecting. Useful for reducing distractions, enforcing bedtime routines, or enhancing network security during off-hours.

### Time Window

The start and end times for the blackout period. WiFi will be disabled between these times on the selected days. End time must be later than start time.

### Days

Select which days of the week the blackout window should be active.`,
  '/wifi/passwords': `## Wi-Fi – Passwords

Manage wireless network passwords. Each entry represents a Wi-Fi network with its own password and security profile controlling access permissions.

### Security Profile

Controls what the connected device can access on the network. Assign a security profile to limit or grant access to specific resources.`,
  '/wifi/passwords/dialog': `## Wi-Fi – Passwords

Each entry represents a Wi-Fi network with its own password and security profile controlling access permissions.

### Label

A descriptive name for this Wi-Fi network, such as "Home" or "Guest Network". This is the network name (SSID) that devices will see.

### Password

The password devices will use to connect. Must be at least 8 characters. Use the generate button to create a strong random password.

### Security Profile

Controls what the connected device can access on the network. Assign a security profile to limit or grant access to specific resources.`,
  '/wifi/settings': `## Wi-Fi – Settings

### Enable Wi-Fi

Turn the wireless radio on or off. When disabled, no devices can connect via Wi-Fi.

### SSID

The SSID (Service Set Identifier) is the name of your WiFi network. It identifies your network to devices, allowing them to find and connect to the network.

### Broadcast

Broadcasting the SSID makes the network discoverable by devices looking for wireless connections. Disabling broadcast hides the network from casual users, adding a layer of security.

### Frequency Band

The frequency band your WiFi operates on. 2.4 GHz offers better range, while 5 GHz offers higher speeds. Choose "Both" to enable dual-band operation.

### Broadcast Separately

When using both frequency bands, this option creates separate SSIDs for 2.4 GHz and 5 GHz (e.g. "MyNetwork" and "MyNetwork-5G"). Useful if you want to explicitly control which band a device connects to.

### Channels

Specific channels within each frequency band. Select "Auto" for the router to choose the best channel, or pick a specific channel to avoid interference from neighboring networks.`,
}

export default HELP_EN
