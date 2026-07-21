# FAQ

Common issues encountered during setup and daily use of StartOS, including network connectivity problems, diagnostic mode, clock sync failures, and service-specific troubleshooting.

## Do I need a surge protector for my server?

Yes. **Always plug your server into a surge protector — never directly into the wall.** Servers are always-on devices, and a single power surge (from lightning, utility events, or appliances cycling on the same circuit) can permanently damage your hardware and corrupt your data drive. A standard surge protector is the minimum requirement. An [uninterruptible power supply (UPS)](surge-and-ups.md) is strongly recommended, as it also protects against brownouts and gives you time to shut down cleanly during a power loss.

## I do not have access to Ethernet

Ethernet is strongly recommended. Servers are always-on, critical devices and should use a wired connection. However, if you do not have access to a router, such as in a work or school environment, there are two options:

- **Server has a WiFi card** (DIY builds only — Start9 servers do not ship with one): Connect a monitor and keyboard to your server (kiosk mode). If no Ethernet interface is detected, you will be prompted to connect to a WiFi network. See [WiFi](wifi.md) for more details.

- **Server does not have a WiFi card**: Use a WiFi extender to bridge the local WiFi network to Ethernet, then connect your server to the extender. The extender below has been tested with StartOS, but others should also work.
  - [TP-Link AC750 WiFi Range Extender](https://www.amazon.com/TP-Link-AC750-WiFi-Range-Extender/dp/B07N1WW638)

## StartOS boots into "Diagnostic Mode"

If you encounter Diagnostic Mode, your best bet is stop clicking and [contact support](https://start9.com/contact).

## During initial setup, I am unable to connect to "start.local".

1. Confirm that the server is plugged into power and Ethernet

1. Confirm that your phone/computer is connected to the same network as the server.

1. Confirm your phone/computer is _not_ connected to a "Guest" network

1. Confirm you are _not_ using the Tor Browser.

1. Confirm your phone/computer is _not_ using a VPN, or that if you are, that it allows LAN connections, such as the examples below:
   - Mullvad - Go to `Settings -> VPN Settings -> Local Network Sharing`
   - ProtonVPN - Go to `Preferences -> Connection -> Allow LAN Connections`

1. Very rarely, your firewall settings may block mDNS. In this case:
   - From your browser, navigate to your router configuration settings. This is usually an IP address such as 192.168.1.1. A simple web search will usually reveal how to access the router configuration settings for a particular brand.
   - Once in the router config settings, find the section that lists the devices on your network. You should see a device labeled `start`. Take note of the associated IP address and enter it into your browser's URL field to enter the setup.

1. Log into your router (the directions for which can be found with a simple web search for your router model and 'how to log in'). Once you are in your router, find the device labeled "start", and visit its associated IP address, which will look something like: `192.168.1.9`

## I am unable to connect to my server's "server-name.local" URL

1. First, try [these steps](#during-initial-setup-i-am-unable-to-connect-to-startlocal). If none resolve the issue, continue below.

1. If your phone/computer is connected to a VPN (including WireGuard/StartTunnel), try disconnecting it. `.local` addresses may not resolve while a VPN is connected — in particular, Android [excludes VPN connections from mDNS resolution](https://source.android.com/docs/core/ota/modular-system/dns-resolver), so a VPN that supplies DNS breaks `.local` even on your home network. If the VPN is StartTunnel, see [this FAQ entry](/start-tunnel/faq.html#why-cant-my-android-phone-resolve-local-addresses-while-the-tunnel-is-on) for the permanent fix: a manual DNS record for your server's `.local` name.

1. Hard refresh the browser:
   - Linux/Windows: `ctrl+shift+R`
   - macOS Firefox: `cmd+shift+R`
   - macOS Safari: `cmd+option+E`, then `cmd+R`

1. Make sure you have successfully followed the [Local Access](lan.md) instructions for your device.

1. If using Firefox from Mac, Windows or Android, ensure you have set `security.enterprise_roots.enable` to `true` in `about:config` per the [instructions](trust-ca.md#3-mozilla-apps-firefox-thunderbird-librewolf)

1. Try connecting using your server's IP address. If this works, it means your issue is specific to `.local`. Try clearing your browser cache and/or restarting your phone/laptop/router. If all fails, try restarting your server.

1. Try connecting using a different browser on the same device. If this works, it means you need to clear cache on your current browser.

1. Try connecting using a different device. If this works, it means you need to clear cache on your current browser and/or restart your current device.

1. Try visiting start.local. Your server may be in diagnostic mode.

1. Try restarting your router.

1. Try restarting your server. Be patient and give it plenty of time to come back online.

## A public domain still loads after disabling it

If you previously had a service interface (or the StartOS UI) publicly accessible on a StartTunnel gateway and then disabled it, your browser may still load the page from cache. Attempting to interact will time out because the port forwarding is still active but the interface is no longer being served.

To resolve this, fully quit and restart your browser. A hard refresh or private window is usually not enough — browsers cache TCP connections more aggressively than page content, and sometimes only a full process restart will clear them.

## Clock Sync Failure

This means your server was unable to sync its clock with the Internet using the Network Time Protocol (NTP). This is usually due to a firewall issue with your network/router. Make sure you are not blocking NTP. If the issue persists, please contact support.

## Issue with a particular service

If a service is misbehaving or crashing, check the logs for that service. Look for any errors that might explain the problem. Often, the solution is to restart the service by clicking "Restart". If the issue persist, [contact support](https://start9.com/contact).
