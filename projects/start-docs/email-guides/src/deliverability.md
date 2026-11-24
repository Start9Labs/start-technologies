# Deliverability

Why mail from a correctly configured server can still land in spam or bounce, what receiving servers actually judge, and how to diagnose a delivery problem.

## Three Things a Receiver Checks

When your mail arrives at Gmail, Outlook.com or Yahoo, the receiving server weighs three kinds of evidence:

1. **Configuration** — whether the sender is set up correctly. These checks pass or fail, and you control all of them.
2. **Reputation** — whether the sender has a history of sending mail people want. You influence it slowly, and only partly.
3. **Content** — whether the message itself looks like spam or phishing.

Configuration is the price of entry: fail it and your mail is refused or filtered no matter what. But passing it only proves your mail is really from you. **Reputation is what gets it into the inbox.**

## Configuration

These are the checks a receiver runs on every message. A self-hosted mail server needs all of them; a hosted provider or relay handles most of them for you, leaving only the DNS records to publish.

| Check                 | What it proves                                                                   | Where it's set                             |
| --------------------- | -------------------------------------------------------------------------------- | ------------------------------------------ |
| **Reverse DNS (PTR)** | The sending IP address has a name, and that name points back to the same address | Whoever owns the IP: your VPS provider     |
| **SPF**               | The sending server is allowed to send for your domain                            | A `TXT` record at your domain              |
| **DKIM**              | The message was signed by your domain and not altered                            | A `TXT` record, plus signing on the server |
| **DMARC**             | SPF or DKIM passed for the domain in the visible From address                    | A `TXT` record at `_dmarc.<your domain>`   |
| **TLS**               | The connection between servers is encrypted                                      | The sending and receiving servers          |

When all of these pass and your mail still goes to spam, configuration is no longer the problem. Going back over your records again won't help.

## Reputation

Receivers keep a score for every sending **IP address** and every sending **domain**. It is built from:

- **History** — how long the address and domain have been sending, and how steadily. A brand-new sender has no history, which large providers treat with suspicion.
- **Recipient behavior** — how often people open, reply to, or mark your mail as spam, or move it out of spam.
- **Blocklists** — whether the address appears on lists such as Spamhaus. Some lists name a single address that sent spam; others name entire ranges, such as home internet connections or a hosting provider's network.
- **Neighbors** — the history of the address range around you. Mail from a range with a lot of past abuse is distrusted before it is read.

Each provider keeps its own score and doesn't publish it. A sender can be welcome at one provider and refused at another on the same day.

### Sending from a home connection

Mail sent directly from a home internet connection almost never reaches inboxes. Most home providers block outgoing port 25 outright. Where they don't, home address ranges are on policy blocklists that mark them as addresses that shouldn't be sending mail directly, and large providers honor those lists. A home address also usually can't have the reverse DNS name receivers expect, because only the internet provider can set it.

### Sending from a VPS

A VPS can send on port 25 (some providers require you to ask first) and lets you set reverse DNS, so the configuration checks can all pass. Reputation is the problem:

- Your address was almost certainly used by someone else before you, and may already be on a blocklist for what they sent.
- VPS ranges are popular with spammers, so the range around your address may be distrusted as a whole. Microsoft in particular blocks whole hosting ranges.
- A new address with no sending history is throttled or filtered until it builds one, which takes weeks of steady, wanted mail — and a few low-volume personal senders never get there.
- Getting off a blocklist means asking each list's operator, some of which require identifying yourself, and some of which list whole networks and won't remove a single address.

None of this is under your control, and it can change without warning. That is why we don't recommend sending directly from your own server for mail that needs to arrive.

### What a relay changes

A **relay** (also called a smarthost or SMTP provider) is a service that delivers your outgoing mail from its own servers, which have an established reputation it works to protect. Your server, or your mail app, hands outgoing mail to the relay; the relay passes it to Gmail, Outlook and the rest. Your domain still signs the mail and still appears as the sender, so recipients see no difference.

Relays police their own customers — limits for new accounts, approval steps, suspension for spam — which is exactly why receivers trust them. A relay is the piece that makes self-hosted mail practical: see [Self-Hosting with Stalwart](self-hosting.md), and [SMTP Credentials](smtp-credentials.md) for setting one up.

## Diagnosing a Delivery Problem

### Check what the receiver saw

Send a message to a Gmail address you can read, open it, and choose **Show original** from the three-dot menu. The top of the page shows **SPF**, **DKIM** and **DMARC**, each marked PASS or FAIL, and the full headers below show the IP address your mail came from. A failure there is a configuration problem you can fix. Three passes and a spam-folder landing is a reputation problem.

### Read the bounce

A rejected message comes back as a bounce with a three-part code and a reason:

- Codes starting **4** (such as `421 4.7.0`) are temporary. The sending server keeps retrying; the receiver is throttling or deferring you, often because it doesn't know you yet.
- Codes starting **5** (such as `550 5.7.1`) are permanent. The message was refused and won't be retried. **`5.7.x`** means a policy rejection: the receiver checked something about you and declined.

The text after the code names the check that failed, and large providers often include a link explaining it. Search for the exact text along with the provider's name.

### Useful tools

- **[MXToolbox](https://mxtoolbox.com/)** checks DNS records, reverse DNS, and whether an IP address is on the common blocklists.
- **[mail-tester.com](https://www.mail-tester.com/)** gives you an address to send a test message to, then scores your configuration and content.
- **[Google Postmaster Tools](https://postmaster.google.com/)** and **[Microsoft SNDS](https://sendersupport.olc.protection.outlook.com/snds/)** show how those providers see your domain or IP, once you send enough mail to them to register.
