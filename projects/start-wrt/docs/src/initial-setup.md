# Initial Setup

After unboxing your Start9 router or completing a fresh install, follow these steps to get up and running. The entire process takes just a few minutes.

## Connect to Wi-Fi

1. Power on the router.

1. On your phone or computer, connect to the `StartWRT` Wi-Fi network using the password printed on the sticker on the bottom of the device.

## Create Your Admin Password

1. A captive portal will open automatically. If it does not, open a browser and navigate to `router.lan`.

1. Create an admin password (minimum 12 characters). This password protects the web interface and is separate from the Wi-Fi password.

1. Confirm the password and click "Submit".

1. Normal Internet access will resume and you will be logged in to the StartWRT web interface.

> [!IMPORTANT]
> All Internet access is blocked until you set an admin password. If you dismiss the captive portal popup, open any browser and navigate to `router.lan`.

> [!NOTE]
> Your timezone is auto-detected from your browser during setup. You can change it later under [Settings](settings.md).

## Trust Your Root CA

To access the web interface securely over HTTPS, download and trust your router's Root Certificate Authority (Root CA).

1. Navigate to `System > Settings > General` and click "Download Root CA".

1. Follow the instructions for [Trusting Your Root CA](trust-ca.md) on each device you want to connect to the router's web interface.

## Explore the Web Interface

The StartWRT web interface is organized into five sections:

- **Internet** — WAN settings, published ports, outbound VPNs
- **Network** — LAN settings, connected devices
- **Security Profiles** — Create and manage access control profiles
- **Points of Entry** — Ethernet ports, Wi-Fi passwords, inbound VPN servers
- **System** — General settings, SSH keys, backups, logs

> [!TIP]
> Toggle **Help Mode** from the header to get a plain-language explanation of everything on the current page, including links to external resources.

## Next Steps

- [Security Profiles](security-profiles.md) — Understand the core concept behind StartWRT
- [Wi-Fi](wifi.md) — Set up additional Wi-Fi passwords for different profiles
- [Settings](settings.md) — Configure timezone, language, and other preferences
