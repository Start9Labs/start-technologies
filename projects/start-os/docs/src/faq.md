# FAQ

Common issues encountered during setup and daily use of StartOS, including a USB installer that will not boot, network connectivity problems, diagnostic mode, clock sync failures, domains that do not resolve, running out of storage, and service-specific troubleshooting.

## Do I need a surge protector for my server?

Yes. **Always plug your server into a surge protector — never directly into the wall.** Servers are always-on devices, and a single power surge (from lightning, utility events, or appliances cycling on the same circuit) can permanently damage your hardware and corrupt your data drive. A standard surge protector is the minimum requirement. An [uninterruptible power supply (UPS)](surge-and-ups.md) is strongly recommended, as it also protects against brownouts and gives you time to shut down cleanly during a power loss.

## I do not have access to Ethernet

Ethernet is strongly recommended. Servers are always-on, critical devices and should use a wired connection. However, if you do not have access to a router, such as in a work or school environment, there are two options:

- **Server has a WiFi card** (DIY builds only — Start9 servers do not ship with one): Connect a monitor and keyboard to your server (kiosk mode). If no Ethernet interface is detected, you will be prompted to connect to a WiFi network. See [WiFi](wifi.md) for more details.

- **Server does not have a WiFi card**: Use a WiFi extender to bridge the local WiFi network to Ethernet, then connect your server to the extender. The extender below has been tested with StartOS, but others should also work.
  - [TP-Link AC750 WiFi Range Extender](https://www.amazon.com/TP-Link-AC750-WiFi-Range-Extender/dp/B07N1WW638)

## StartOS boots into "Diagnostic Mode"

If you encounter Diagnostic Mode, your best bet is stop clicking and [contact support](https://start9.com/contact).

## My server boots into StartOS instead of the USB installer

A Start9 server boots from a USB installer on its own, with nothing to press and no setting to change. If yours starts StartOS as usual with the installer plugged in, the USB drive is almost always the problem:

1. Confirm the drive was **flashed** with the StartOS `.iso` in balenaEtcher, as described in [Flash](installing-startos.md#flash). Formatting a drive and copying the file onto it produces a drive that will not boot. The [Server Pure](firmware-pure.md) and [Server One (2023)](firmware-one-2023.md) firmware pages describe a different procedure for a different purpose; do not use them to install or update StartOS.

1. Flash the drive again, and use a different USB drive if you have one. Plug it into a USB 3 port, typically blue or marked "SS".

If a freshly flashed drive still does not boot, connect a monitor and keyboard and pick the drive from the boot menu:

- **Server Pure** — Needs no boot menu: the firmware boots a USB drive whenever one is plugged in. If a freshly flashed drive still does not boot, [contact support](https://start9.com/contact).

- **Server One (2023)** — Press `F10` repeatedly from the moment you power on, and select the USB drive from the boot menu that appears. If StartOS starts before the menu appears, use the power button instead: with the server off, hold the power button for three seconds and release it. A menu of keys appears; press `F10` there.

- **Server One (2024)** — Press `Del` repeatedly from the moment you power on to enter the BIOS. Under **Boot**, open **Boot Option Priorities** and set **Boot Option #1** to the USB drive, then press `F4` to save and restart.

For other hardware, see the [install guide](installing-startos.md#install) and the [Community Hub](https://community.start9.com).

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

1. If your phone/computer is on a VPN (including WireGuard/StartTunnel), `.local` can fail to resolve while it's connected — Android in particular [excludes VPN connections from mDNS resolution](https://source.android.com/docs/core/ota/modular-system/dns-resolver). Over StartTunnel this is handled for you: a connected StartOS server automatically injects a DNS record for its `.local` name (as long as DNS injection is enabled for that server — the default), so it resolves through the tunnel. If injection is turned off, disconnect the VPN or add a manual record — see [this StartTunnel FAQ entry](/start-tunnel/faq.html#why-cant-my-android-phone-resolve-local-addresses-while-the-tunnel-is-on).

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

## I am unable to reach a service at its domain name

Whether the domain is [public](clearnet.md) or [private](private-domains.md), start by asking DNS directly, before clearing caches or toggling settings. The answer, and which server gave it, tells you which case below you are in. On the device that cannot connect, open a terminal and run:

```
nslookup service.example.com
```

The `Server:` line is the resolver your device actually used, and the address underneath is its answer. Then ask a public resolver the same question:

```
nslookup service.example.com 1.1.1.1
```

If you would rather not use a terminal, paste the domain into [dnschecker.org](https://dnschecker.org) for the public half; it asks many public resolvers at once.

- **The public resolver returns your gateway's address, but your own resolver returns nothing, `0.0.0.0`, or a different address.** The resolver your device is using is filtering the name; the browser typically shows a `DNS_PROBE_…` error while other sites load fine. The usual culprits are the ad, tracker or malware blocking built into privacy VPN apps (ProtonVPN's NetShield, Mullvad's DNS content blocking, NordVPN's Threat Protection), a Pi-hole or AdGuard Home on your network, a filtering DNS service such as NextDNS, or a browser's "secure DNS" setting. Turn the feature off or allow your domain in it. Your server is not involved.

- **Neither resolver returns anything, and the domain is public.** The DNS record is missing or has not propagated yet, which can take a few hours. Check the record at your registrar against [Set Up DNS Records](clearnet.md#set-up-dns-records), and re-run the test from the address's **Settings** on the service's [Interfaces](interfaces.md) tab.

- **The domain is private.** A private domain has no public record, so a public resolver never answers for it. It resolves only through the DNS server of the gateway it was added to: on an Ethernet or WiFi gateway your router must hand out StartOS as its DNS server, and on a StartTunnel gateway your device must be connected to the tunnel _and_ using the tunnel's resolver, with [DNS injection](/start-tunnel/dns-records.html) enabled for the server (the default). A phone runs only one VPN at a time, so turning on a privacy VPN such as ProtonVPN or Mullvad disconnects your StartTunnel tunnel and takes its resolver with it; private domains stop resolving until you switch back. If your device is connected and still gets no answer, check that the record appears on StartTunnel's DNS Records page, then re-import a freshly generated config for the device: configs generated before StartTunnel 1.1.0 have no `DNS =` line, so the device keeps asking its usual resolver. If the private domain is also a real domain with public records, a device that is not using the gateway's resolver gets the public answer instead, which can look like the name works while it points somewhere else entirely.

- **The answer is the right address, but the page still does not load.** The right address is your gateway's public IP for a public domain, and your server's LAN IP or tunnel address for a private one. DNS is not the problem. For a private domain, confirm the device is actually on that network (the tunnel is connected, or you are on the LAN) and that you have [trusted your Root CA](trust-ca.md). For a public domain, run the port-forwarding test from the address's **Settings** on the service's [Interfaces](interfaces.md) tab.

## A public domain still loads after disabling it

If you previously had a service interface (or the StartOS UI) publicly accessible on a StartTunnel gateway and then disabled it, your browser may still load the page from cache. Attempting to interact will time out because the port forwarding is still active but the interface is no longer being served.

To resolve this, fully quit and restart your browser. A hard refresh or private window is usually not enough — browsers cache TCP connections more aggressively than page content, and sometimes only a full process restart will clear them.

## Clock Sync Failure

This warning means your server has not completed a successful time sync over the Internet (NTP) since it booted. The clock itself is usually still correct — it is set from the onboard hardware clock at boot — so services generally continue to run normally. Ongoing sync still matters (certificate validation, logs, and some services are time-sensitive), so the warning should not be ignored indefinitely.

The usual cause is the network path, not the server: a router or firewall blocking outbound NTP (UDP port 123), an ISP filtering it, or the public `pool.ntp.org` servers assigned to your region being unreliable.

To diagnose, [connect via SSH](ssh.md) and run:

```
timedatectl timesync-status
sudo journalctl -u systemd-timesyncd -b
```

The journal names each time server StartOS is trying and how the attempts fail (for example `Timed out waiting for reply`).

If the default servers are unreachable from your network, point StartOS at servers that do work from it (for example Google's and Cloudflare's):

```
sudo mkdir -p /etc/systemd/timesyncd.conf.d
printf '[Time]\nNTP=time.google.com time.cloudflare.com\n' | sudo tee /etc/systemd/timesyncd.conf.d/10-custom.conf
sudo systemctl restart systemd-timesyncd
```

Within a minute or so, `timedatectl timesync-status` should report a successful sync. If the warning in the UI does not clear shortly after that, restart your server once. The override file survives reboots; after a StartOS update, check that it is still present and re-create it if needed.

If time sync still fails, please [contact support](https://start9.com/contact).

## Can I add a second drive to give a service more storage?

Not yet. StartOS keeps every service's data on a single data drive, the one chosen at the Select Drives step when [installing StartOS](installing-startos.md). There is no way to attach an additional drive to one service, and a drive plugged in after setup is only recognized as a [backup](backup-create.md) target.

If a service is running low on space, you have two options today:

- **Move to a larger data drive.** Install StartOS with the new drive as the data drive, then choose **Transfer** at [initial setup](initial-setup.md) and select your old data drive to move everything across. Keep the old drive connected until the transfer completes, and never boot from it as a StartOS server again.

- **Keep large files on storage the service can reach over the network.** Some services can use storage outside your server on their own. Nextcloud, for example, can attach an SMB share, a WebDAV server or an S3 bucket through its built-in External Storage app, so a large library can live on a NAS or another computer. Check the service's own instructions for what it supports. Linking one service's files into another, such as File Browser into Nextcloud or Immich, does not add space, since those files are on the same data drive.

Support for multiple drives is planned for StartOS 0.4.1. There is no release date yet.

## Issue with a particular service

If a service is misbehaving or crashing, check the [logs](logs.md) for that service — open the service and select its **Logs** tab. Look for any errors that might explain the problem. Often, the solution is to restart the service by clicking "Restart". If the issue persist, [contact support](https://start9.com/contact).
