# Initial Setup

After unboxing your Start9 router or completing a fresh install, follow these steps to get up and running. The entire process takes just a few minutes.

## Connect the Router

1. Connect your modem or upstream router to the Ethernet port labeled **WAN**, and the included switch to the port labeled **LAN**, for wired devices on your network.

1. Power on the router.

1. Connect your phone or computer to the router, either:
   - **Over Wi-Fi:** join the `StartWRT` network using the password printed on the sticker on the bottom of the device.
   - **Over Ethernet:** plug it into the switch.

> [!NOTE]
> Routers sold in the United States ship without Wi-Fi. Set up over Ethernet, and see [Installing the Wi-Fi Module](wifi-module.md) to add it.

## Create Your Admin Password

1. A captive portal will open automatically. If it does not, open a browser and navigate to `router.lan`.

1. Create an admin password (minimum 12 characters). This password protects the web interface and is separate from the Wi-Fi password.

1. Confirm the password and click "Set Password".

1. The page will confirm "Setup complete" and normal Internet access will resume. Click "Open router.lan" to continue to [Trust Your Root CA](#trust-your-root-ca). If it opens inside the captive portal popup, open `router.lan` in your browser instead to download the certificate.

> [!IMPORTANT]
> All Internet access is blocked until you set an admin password. If you dismiss the captive portal popup, open any browser and navigate to `router.lan`.

> [!NOTE]
> Your timezone is auto-detected from your browser during setup. You can change it later under [Settings](settings.md).

## Trust Your Root CA

To access the web interface securely over HTTPS, download and trust your router's Root Certificate Authority (Root CA).

1. Open `router.lan` in your browser and follow the steps to download your Root CA.

1. Follow the instructions for [Trusting Your Root CA](trust-ca.md) on each device you want to connect to the router's web interface.

## Explore the Web Interface

The StartWRT web interface is organized into five sections:

- **Internet** — [WAN settings](wan.md), [published ports](published-ports.md), [outbound VPNs](outbound-vpn.md)
- **Network** — [LAN settings](lan.md), [connected devices](devices.md)
- **Security Profiles** — [Create and manage access control profiles](security-profiles.md)
- **Points of Entry** — [Ethernet ports](ethernet.md), [Wi-Fi passwords](wifi.md), [inbound VPN servers](inbound-vpn.md)
- **System** — [General settings](settings.md), [SSH keys](ssh.md), [backups](backups.md), [logs](settings.md#logs)

> [!TIP]
> Toggle **Help** in the header to get a plain-language explanation of everything on the current page, including links to external resources.

## Next Steps

- [Security Profiles](security-profiles.md) — Understand the core concept behind StartWRT
- [Wi-Fi Settings](wifi.md#settings) — Select your country so the router uses the Wi-Fi channels and transmit power permitted there
- [Wi-Fi](wifi.md) — Set up additional Wi-Fi passwords for different profiles
- [Settings](settings.md) — Configure timezone, language, and other preferences
