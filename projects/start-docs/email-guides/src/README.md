# Email Guides

How email works today, what it takes to get your mail delivered, and which setup fits your goals — from a hosted mailbox on your own domain to running your own mail server on StartOS.

## Why This Guide Exists

Email is the one protocol almost everyone still uses that no single company owns. Anyone can, in principle, run a mail server and exchange mail with the rest of the world. In practice, a handful of providers now decide whose mail reaches an inbox, and the rules they apply are invisible from the outside. People who set out to "own their email" often discover this the hard way: their server works, their records check out, and their messages still land in spam or bounce.

This guide explains why, and lays out the realistic options. Most people's goals — an address they control, privacy from advertisers and data brokers, independence from any one company — can be met without running a mail server at all. For those who do want their mail on their own hardware, StartOS makes it practical, provided outgoing mail is sent the right way.

## Quick Answer

| Your goal                                                      | Start here                                                                              |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| An address you own, mail that arrives, minimal upkeep          | [Hosted Email on Your Domain](hosted-email.md)                                          |
| Your mailbox stored on your own server                         | [Self-Hosting with Stalwart](self-hosting.md)                                           |
| Mail that can't easily be tied to your identity                | [Email and Privacy](privacy.md#anonymity)                                               |
| Services on StartOS (Nextcloud, Vaultwarden, …) that send mail | [SMTP Credentials](smtp-credentials.md)                                                 |
| Help deciding                                                  | [Email Setups Compared](setups-compared.md), then [Recommendations](recommendations.md) |

## What's in This Guide

### Understanding Email

- **[How Email Works](how-email-works.md)** — The pieces every message passes through, and the short history of how an open protocol came to be run by a few large providers.

- **[Deliverability](deliverability.md)** — Why correct records are not enough, how receiving servers judge reputation, why mail sent directly from a home or VPS address struggles, and how to read a bounce.

- **[Email and Privacy](privacy.md)** — Who can read your mail in each setup, what metadata reveals, what end-to-end encryption does and doesn't cover, and what anonymity takes.

### Choosing a Setup

- **[Your Own Domain](your-own-domain.md)** — Why an address at your own domain is what actually makes you independent, and what owning one involves.

- **[Email Setups Compared](setups-compared.md)** — Every common setup side by side: who holds your mail, whether it arrives, what it costs and how much work it is.

- **[Recommendations](recommendations.md)** — What we suggest for individuals, families, small businesses, people who need anonymity, and people who want to run it all themselves.

### Setting Up

- **[Hosted Email on Your Domain](hosted-email.md)** — Your domain on a privacy-focused provider, read and sent from Thunderbird on your computer and phone.

- **[Self-Hosting with Stalwart](self-hosting.md)** — How a mail server on StartOS fits together: receiving through StartTunnel, sending through a relay, and what you take on.

- **[SMTP Credentials](smtp-credentials.md)** — Getting sending credentials from Forward Email, Amazon SES, Gmail, Mailgun, Proton Mail or SendGrid, for StartOS, a Stalwart relay or any app.

- **[Moving Your Mail](moving-your-mail.md)** — Bringing your old mail along, and running your old and new addresses side by side while you switch.

## Learn More

- [StartOS Documentation](/start-os/) — Installing StartOS, user manual, networking, backups
- [StartTunnel Documentation](/start-tunnel/) — Giving your server a public IP address
- [Get Help](https://start9.com/help) — Help from the community and the Start9 team
