# Email Setups Compared

Every common way to run your email, side by side: who holds your mail, whether it reliably arrives, what it costs, and how much work it takes.

## At a Glance

| Setup                                                                        | Your own address | Mail arrives reliably | Who holds your mailbox   | Effort  | Typical cost           |
| ---------------------------------------------------------------------------- | ---------------- | --------------------- | ------------------------ | ------- | ---------------------- |
| [Free large provider](#free-large-provider)                                  | No               | Yes                   | The provider             | None    | Free                   |
| [Privacy provider, its own address](#privacy-provider-its-own-address)       | No               | Yes                   | The provider (encrypted) | None    | Free or a few $/month  |
| [Forwarding only](#forwarding-only)                                          | Yes              | Yes                   | Your existing provider   | Low     | Free                   |
| [Your domain on a hosted provider](#your-domain-on-a-hosted-provider)        | Yes              | Yes                   | The provider             | Low     | Domain + a few $/month |
| [Self-hosted, sending through a relay](#self-hosted-sending-through-a-relay) | Yes              | Yes                   | You                      | High    | Domain + VPS + relay   |
| [Self-hosted, sending directly](#self-hosted-sending-directly)               | Yes              | **No**                | You                      | Highest | Domain + VPS           |

## Free Large Provider

Gmail, Outlook.com, iCloud Mail, Yahoo.

- **Good:** free, reliable, excellent spam filtering, every app supports them.
- **Bad:** the provider reads and stores everything in readable form, and you're subject to its terms. The address isn't yours: losing the account loses the address.

## Privacy Provider, Its Own Address

Proton Mail (`proton.me`), Tuta (`tuta.com`).

- **Good:** stored mail is encrypted so the provider can't read it; end-to-end encryption with other users of the same service; can be signed up for anonymously. The best option for [anonymity](privacy.md#anonymity).
- **Bad:** the address still belongs to the provider. Using a desktop mail app takes extra steps: Proton's IMAP access needs its Bridge app on a paid plan, and Tuta works only in its own apps.

## Forwarding Only

A forwarding service (Forward Email's free plan, among others) receives mail at your domain and passes it on to an existing mailbox, such as your Gmail.

- **Good:** you get your own address immediately, keep the inbox you're used to, and can later switch to full hosting without changing the address.
- **Bad:** your mail still ends up at your existing provider, and replies go out from that provider's address unless you add sending (which is no longer free).

## Your Domain on a Hosted Provider

Forward Email, Proton (custom domain), Fastmail and others host full mailboxes on your domain, which you read in their webmail or in any mail app.

- **Good:** an address you own, mail that arrives, no servers to run. With a privacy-focused provider, stored mail is encrypted so the provider can't read it. Switching provider later is a DNS change.
- **Bad:** a company still holds your mailbox, however well encrypted, and handles each message in readable form as it passes through.

This is what we recommend for most people. See [Hosted Email on Your Domain](hosted-email.md).

## Self-Hosted, Sending Through a Relay

Stalwart on StartOS stores your mail and receives it directly from other servers through a public IP address on a StartTunnel VPS. Outgoing mail is handed to a relay, which delivers it from servers with an established reputation.

- **Good:** your mailbox, contacts and calendars live on your own hardware; no company holds your correspondence history. Delivery is as reliable as the relay's.
- **Bad:** you run a VPS, a domain, DNS records and the server, and keep all of them up. The relay sees every message you send. Spam filtering, backups and security are yours.

See [Self-Hosting with Stalwart](self-hosting.md).

## Self-Hosted, Sending Directly

The same server, but delivering outgoing mail itself instead of through a relay.

- **Good:** no third party handles your outgoing mail.
- **Bad:** your mail's arrival depends on the reputation of your server's IP address, which you can't control. From a home connection it generally won't arrive at all; from a VPS it often lands in spam or is refused, especially at Microsoft. See [Deliverability](deliverability.md).

We don't recommend this for mail that needs to arrive.

## Mail From Your Services

Separate from your personal mail, services on StartOS — Nextcloud, Vaultwarden, Gitea and others — send notifications and password resets. They need **SMTP credentials** for an account that can send mail. Any of the setups above that includes sending can provide them, or you can use a dedicated sending service. See [SMTP Credentials](smtp-credentials.md).
