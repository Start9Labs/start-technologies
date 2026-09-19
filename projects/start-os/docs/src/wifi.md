# WiFi

StartOS supports connecting your server to a wireless network when a wired Ethernet connection is not available. WiFi is managed from **System > WiFi**.

> [!WARNING]
> A wired Ethernet connection provides significantly better reliability, throughput, and latency. WiFi is prone to intermittent drops that can interrupt backups, corrupt data transfers, and degrade service performance. **Only use WiFi if Ethernet is truly not an option.**

## Requirements

Your server must have a compatible wireless interface. The Raspberry Pi 4 image supports the board's built-in wireless interface. If no wireless hardware is detected, the WiFi page will display "No wireless interface detected" and no controls will be available.

> [!NOTE]
> Servers sold by Start9 do not come with wireless networking cards, but a wireless card can be added to any of the servers we sell.

## Enabling WiFi

WiFi is managed from **System > WiFi**. Use the toggle at the top of the page to enable or disable the wireless radio. When disabled, no networks will be scanned or connected.

## Connecting to a network

Once WiFi is enabled, two lists are displayed:

- **Known Networks** - Networks you have previously saved. Tap one to reconnect. Use the trash icon to forget a saved network.
- **Other Networks** - Available networks detected nearby. Tap one to connect. If the network is secured, you will be prompted for the password.

Signal strength is indicated by the WiFi icon color: green (strong), yellow (moderate), or red (weak). A lock icon means the network requires a password.

## Adding a hidden network

To connect to a network that does not broadcast its SSID, tap the **Add** button. Enter the network SSID and password, then choose:

- **Save for later** - Saves the credentials without connecting immediately.
- **Save and connect** - Saves the credentials and connects right away.

## Fix WiFi Connection Issues

If a connection attempt fails, you will see a "Failed to connect" warning. Double-check the password and try again. The system retries several times before reporting failure, so the process may take a moment.
