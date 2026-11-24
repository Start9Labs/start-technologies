# Self-Hosting with Stalwart

How a mail server on StartOS fits together — receiving through a public address, sending through a relay — and what you take on by running one.

## Is This for You?

Self-hosting puts your mailbox, contacts and calendars on your own hardware, so no company holds your correspondence history. In exchange, you keep a server, a VPS, a domain and its DNS records running, and when something breaks, there's no provider to fix it. If what you want is an address you own and mail that arrives, [Hosted Email on Your Domain](hosted-email.md) gets you there with far less work.

## How It Fits Together

A self-hosted setup on StartOS has four parts:

- **[Stalwart](https://github.com/Start9Labs/stalwart-startos/blob/master/instructions.md)** on your StartOS server: the mail server. It receives and stores mail, serves it to your apps over IMAP and JMAP, and holds calendars and contacts over CalDAV and CardDAV. Everything lives in one database that StartOS backs up.
- **A public IP address** for other mail servers to deliver to. Home connections rarely work: most block port 25, and many share one address among several customers. A [StartTunnel](/start-tunnel/) gateway on a VPS gives your server a public address of its own, without revealing your home address.
- **A relay** that delivers your outgoing mail from servers with an established reputation. See [Deliverability](deliverability.md) for why sending directly from your own address isn't reliable, and [SMTP Credentials](smtp-credentials.md) for getting relay credentials.
- **Your domain's DNS records**, pointing incoming mail at your public address and authorizing your server and your relay to send. Stalwart's **Show DNS Records** action lists them.

Mail flows like this:

- **Incoming:** a sender's server looks up your MX record, connects to port 25 on your StartTunnel VPS, and the tunnel carries the connection to Stalwart.
- **Outgoing:** your mail app hands the message to Stalwart; Stalwart signs it with your domain's DKIM key and hands it to the relay; the relay delivers it.
- **Reading:** your apps connect to Stalwart over the same public address — or over your local network or a VPN.

## Setting It Up

Stalwart's instructions on StartOS walk through every step: choosing the gateway, publishing the mail ports, the DNS records and the relay. Install Stalwart from the StartOS Marketplace and follow its **Instructions** tab, or read them [on GitHub](https://github.com/Start9Labs/stalwart-startos/blob/master/instructions.md).

> [!TIP]
> Set it up on a spare domain first. Move your main address over once the server has been running reliably for a few weeks.

## What You Take On

- **Uptime.** If your server, your home connection, the VPS or your DNS goes down, so does your mail. Short outages lose nothing: senders' servers keep messages and retry, typically for several days. Longer outages bounce mail back to its senders.
- **Backups.** Your mail exists only on your server and in your backups. Keep a backup off the server, and test restoring it.
- **Security.** Your server holds everyone's mail on the domain, and its mail ports face the internet. Keep StartOS and Stalwart updated, use strong passwords and two-factor sign-in in Stalwart's Account Manager, and keep the Admin Console off the public internet (Stalwart on StartOS does this by default).
- **Spam.** Stalwart filters incoming spam itself. Expect to tune it at first.
- **Your relay and VPS accounts.** Both are third parties in the path of your mail. The relay handles every message you send; the VPS provider sees the encrypted traffic to and from your server.

## Outages at Big Providers

When Microsoft or Google has an outage, your own server keeps working: you can still read your mail and send messages. Mail to people at the affected provider waits in the queue and is delivered when it recovers, and their mail to you arrives late for the same reason. You've traded the large provider's points of failure for your own, not removed them.
