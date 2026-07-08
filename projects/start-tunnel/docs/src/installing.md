# Installing

Install StartTunnel on a Debian VPS by renting a server, running the one-line installer script, and initializing the web interface. The entire process takes just a few minutes.

## Watch The Video

<div class="yt-video" data-id="JGhBFZ0hNOU" data-title="Installing StartTunnel"></div>

## Prerequisites

Before setting up a VPS, make sure you have an SSH key pair on your laptop or desktop — this is how you'll log into the server. Most providers require you to upload your public key during server creation. If you don't have one yet, open a terminal on your computer and run:

```bash
ssh-keygen -t ed25519
```

Your public key will be at `~/.ssh/id_ed25519.pub`. You'll paste its contents into your VPS provider's dashboard when creating the server.

## Get a VPS

Rent a cheap VPS with a dedicated public IP. Minimum CPU/RAM/disk is fine. For bandwidth, no need to exceed your home Internet's upload speed.

### Requirements

- Debian 13
- Root access
- Dedicated public IPv4 address (required for clearnet port forwarding)

> [!IMPORTANT]
> StartTunnel is designed to be the sole application on your VPS. The installer disables UFW and manages its own firewall rules via iptables. Do not run other Internet-facing services on the same VPS.

> [!WARNING]
> Port forwarding requires a **dedicated public IPv4 address** assigned to your VPS. Shared IPv4 addresses (CGNAT, shared NAT, or load-balanced IPs) will not work. IPv6-only VPSes will not work for clearnet hosting either — see [Can I use an IPv6-only VPS?](faq.md#does-starttunnel-work-on-an-ipv6-only-vps) in the FAQ. Confirm with your VPS provider that the IPv4 address is dedicated to your VM before purchasing.

### Cloud firewalls

Some VPS providers have a **cloud-panel firewall** that sits outside the operating system. This firewall can silently block WireGuard traffic (UDP 51820) before it ever reaches your VPS, even if the OS firewall is correctly configured. If your provider is listed below, you must open UDP 51820 in the cloud panel **before** devices can connect.

> [!NOTE]
> Providers like Hetzner, DigitalOcean, Vultr, and Linode have optional cloud firewalls that are **not** enabled by default. If you haven't explicitly enabled one, no action is needed.

{{#tabs global="cloud-firewall" }}
{{#tab name="IONOS" }}

IONOS VPSes have a hardware-level firewall that by default only allows TCP 22, 80, 443, 8443, and 8447. UDP 51820 is blocked before it reaches the VM.

1. Log into the [IONOS Cloud Panel](https://my.ionos.com).
1. Go to **Server & Cloud** → select your VPS → **Network** → **Firewall Policies**.
1. Add an inbound rule: **Protocol** = UDP, **Port** = 51820.
1. Save and apply.

{{#endtab }}
{{#tab name="Oracle Cloud" }}

Oracle Cloud has **two** firewalls that both need to be opened: the VCN Security List (cloud-level) and OS-level iptables rules pre-installed on the image.

**Cloud-level (Security List):**

1. In the OCI Console, go to **Networking** → **Virtual Cloud Networks** → select your VCN.
1. Select your subnet's **Security List**.
1. Add an **Ingress Rule**: Source CIDR = `0.0.0.0/0`, IP Protocol = UDP, Destination Port Range = `51820`.

**OS-level (if using Oracle Linux):**

```bash
sudo firewall-cmd --add-port=51820/udp --permanent
sudo firewall-cmd --reload
```

> [!WARNING]
> Both layers must be opened. Opening only one will still block WireGuard.

{{#endtab }}
{{#tab name="AWS" }}

EC2 instances use **Security Groups** that deny all inbound traffic by default.

1. In the EC2 Console, go to your instance → **Security** → click the **Security Group**.
1. Under **Inbound rules**, click **Edit inbound rules**.
1. Add a rule: **Type** = Custom UDP, **Port range** = 51820, **Source** = 0.0.0.0/0.
1. Save.

{{#endtab }}
{{#tab name="Google Cloud" }}

GCE uses **VPC Firewall Rules** that only allow TCP 22, TCP 3389, and ICMP by default.

1. In the Cloud Console, go to **VPC Network** → **Firewall**.
1. Click **Create Firewall Rule**.
1. Set: Direction = Ingress, Targets = All instances (or a specific network tag), Source = `0.0.0.0/0`, Protocol = UDP, Port = `51820`.
1. Save.

{{#endtab }}
{{#tab name="Azure" }}

Azure VMs get a **Network Security Group** that only allows SSH (TCP 22) by default.

1. In the Azure Portal, go to your VM → **Networking**.
1. Click **Add inbound port rule**.
1. Set: Source = Any, Destination port = `51820`, Protocol = UDP, Action = Allow.
1. Save.

{{#endtab }}
{{#endtabs }}

## Connect to your VPS

{{#tabs global="ssh-auth" }}
{{#tab name="SSH key (most providers)" }}

Most providers let you add an SSH public key during server creation. If you did, connect with:

```bash
ssh root@<VPS_IP>
```

{{#endtab }}
{{#tab name="Root password" }}

Some providers (notably IONOS standard VPS) only provide a root password at provisioning — no SSH key option. If your provider emailed or displayed a root password, connect with:

```bash
ssh root@<VPS_IP>
```

Enter the password when prompted.

{{#endtab }}
{{#endtabs }}

## Run the installer

Run:

```bash
curl -sSL https://start9.com/start-tunnel/install.sh | sh
```

> [!NOTE]
> If DNS resolution is not working on your VPS, the installer will configure public DNS resolvers (Google, Cloudflare, Quad9) and back up your existing `/etc/resolv.conf`.

## Initialize the web interface

StartTunnel can be fully managed from the command line, but it also offers a web UI for convenience. To set it up, run:

```bash
start-tunnel web init
```

This initializes a web server, creates a random password, and configures an SSL certificate. You will be prompted to either generate a new Root CA or provide your own certificate.

Save the URL and password to your password manager.

> [!NOTE]
> The URL, password, and certificate are only for accessing your StartTunnel's web user interface. None are needed to use StartTunnel from the command line.

### Certificate options

When prompted for a certificate, you have two choices:

{{#tabs global="tunnel-cert" }}
{{#tab name="Use your StartOS Root CA (recommended)" }}

If you already have a StartOS server and have [trusted its Root CA](/start-os/trust-ca.html), you can sign the StartTunnel certificate with that same CA. This means your browser will trust the StartTunnel web UI automatically — no additional certificate to manage.

1. On your StartOS server, generate a certificate for your StartTunnel's hostname or IP:

   ```bash
   start-cli net ssl generate-certificate <HOSTNAME_OR_IP>
   ```

   This outputs a private key and certificate chain in PEM format.

1. During `start-tunnel web init`, when prompted for a certificate, select **Provide**.

1. Paste the **private key** first and press Enter. You may need to press Enter an extra time for it to be accepted.

1. Paste the **certificate chain** next and press Enter. Again, you may need to press Enter an extra time.

{{#endtab }}
{{#tab name="Generate a new Root CA" }}

Select **Generate** when prompted. StartTunnel will create its own Root CA and use it to sign a certificate. The Root CA will be printed to the console.

Trust the Root CA on each device that will access the web UI. Select your operating system:

{{#tabs global="platform" }}
{{#tab name="Mac" }}

1. Open the Terminal app.

1. Type or paste the following command (**do not** press Return yet):

   ```
   pbpaste > ~/Desktop/tunnel-ca.crt
   ```

   Again, **do not** press Return yet.

1. Copy your Root CA to clipboard from the console output (including `-----BEGIN CERTIFICATE-----` and `-----END CERTIFICATE-----`). **do not** paste it anywhere.

1. Back in Terminal, just press Return. The file `tunnel-ca.crt` is saved to your Desktop.

1. [Trust the certificate](/start-os/trust-ca.html?platform=Mac).

{{#endtab }}
{{#tab name="Windows" }}

1. Open the Notepad app.

1. Copy and paste your Root CA from the console output (including `-----BEGIN CERTIFICATE-----` and `-----END CERTIFICATE-----`).

1. Save the file as `tunnel-ca.crt` (plaintext).

1. [Trust the certificate](/start-os/trust-ca.html?platform=Windows).

{{#endtab }}
{{#tab name="Linux" }}

1. Open a text editor (gedit, nano, etc.).

1. Copy and paste your Root CA from the console output (including `-----BEGIN CERTIFICATE-----` and `-----END CERTIFICATE-----`).

1. Save the file as `tunnel-ca.crt` (plaintext).

1. [Trust the certificate](/start-os/trust-ca.html?platform=Linux).

{{#endtab }}
{{#tab name="Android / Graphene" }}

1. Save the Root CA to a `tunnel-ca.crt` file on your computer (see Mac, Windows, or Linux tabs).

1. Send the `tunnel-ca.crt` file to your phone (email, messaging app, etc.).

1. [Trust the certificate](/start-os/trust-ca.html?platform=Android+%2F+Graphene).

{{#endtab }}
{{#tab name="iOS" }}

1. Save the Root CA to a `tunnel-ca.crt` file on your computer (see Mac, Windows, or Linux tabs).

1. Send the `tunnel-ca.crt` file to your phone (email, messaging app, etc.).

1. [Trust the certificate](/start-os/trust-ca.html?platform=iOS).

{{#endtab }}
{{#endtabs }}

{{#endtab }}
{{#endtabs }}

## Next steps

- [Subnets](subnets.md) — Create isolated VLANs
- [Devices](devices.md) — Add servers, phones, and laptops
- [Port Forwarding](port-forwarding.md) — Expose ports on your VPS's public IP
