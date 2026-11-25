# Your Own Domain

Why an address at a domain you own is what actually makes you independent of any mail provider, and what owning one involves.

## Independence Is the Address, Not the Server

An address like `you@gmail.com` belongs to Google. If Google closes your account, changes its terms, or you simply want to leave, the address stays behind — and with it every account, contact and correspondent that knows you by it.

An address like `you@yourname.com` belongs to you. The provider that hosts it is replaceable: point your domain's records at a different provider, or at your own server, and the same address keeps working. This is the independence most people are after when they say they want to "own their email," and it doesn't require running anything yourself.

## What You Need

- **A domain**, registered with a domain registrar (Porkbun, Namecheap, Cloudflare and many others) for a yearly fee, typically $10–$20 for common endings like `.com`. Renew it on time, with auto-renew on: a domain that lapses can be bought by someone else, who then receives your mail.
- **DNS hosting**, where your domain's records live. Most registrars include it.
- **A mail host** for the domain: a hosted provider, or your own server. See [Email Setups Compared](setups-compared.md).

## The Records Mail Uses

Whoever hosts your mail tells you exactly which records to create; you add them at your DNS host. You'll meet these:

| Record           | Type             | What it does                                              |
| ---------------- | ---------------- | --------------------------------------------------------- |
| **MX**           | `MX`             | Names the server that receives mail for your domain       |
| **SPF**          | `TXT`            | Lists the servers allowed to send mail for your domain    |
| **DKIM**         | `TXT` or `CNAME` | Publishes the key that verifies your mail's signatures    |
| **DMARC**        | `TXT`            | Tells receivers how to treat mail that fails SPF and DKIM |
| **Verification** | `TXT`            | Proves to a provider that you control the domain          |

A domain has **one** SPF record. When you add a second sender — a relay, a newsletter service — add its `include:` to the existing record rather than creating another one; two SPF records make both invalid.

## Choosing a Domain

- **Your name, or something you'll keep for life.** Changing domains later means changing your address everywhere, which is what this is meant to avoid.
- **A common ending** (`.com`, `.net`, `.org`, or your country's) is trusted more readily by spam filters than an unusual one.
- **Use a separate domain for experiments.** If you want to try self-hosting, a spare domain keeps a mistake from affecting the address everyone knows.

## Switching Providers Later

Because the address is yours, moving to another provider is a change of DNS records:

1. Set up the domain and mailboxes at the new provider.
2. Change the MX record (and SPF, DKIM and DMARC) to the new provider's values.
3. Move your existing mail across. See [Moving Your Mail](moving-your-mail.md).

Mail sent while DNS changes spread is not lost: it goes to whichever provider the sender's server finds, so check both mailboxes for a few days.
