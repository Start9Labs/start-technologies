# How Email Works

The handful of pieces every message passes through, and how an open protocol that anyone could run came to be governed by a few large providers.

## The Pieces

Email splits into two jobs that are easy to confuse: **delivering** mail between servers, and **reading** it from your devices.

- **Your domain** is the part after the `@`. Its DNS records tell the world which server accepts mail for it and which servers are allowed to send on its behalf.
- **A mail server** does the delivering. It accepts mail addressed to your domain, stores it in your mailbox, and hands your outgoing mail to the recipient's server.
- **A mail app** (Thunderbird, Apple Mail, a webmail page) does the reading. It connects to your mail server, shows you your mailbox, and passes the messages you write to the server for delivery. The app never talks to the recipient directly.

Between servers, mail travels over **SMTP** (Simple Mail Transfer Protocol). Your app sends your outgoing mail to your own server over SMTP too, after signing in. To read, your app uses **IMAP**, which keeps the mailbox on the server and syncs every device to it, or the newer **JMAP**, which does the same job more efficiently.

## What Happens When You Send

1. You write a message in your app and press Send. The app signs in to your mail server and hands it over.
2. Your server looks up the recipient's domain in DNS. Its **MX record** names the server that accepts mail for that domain.
3. Your server connects to that server on port 25 and offers the message.
4. The receiving server checks who is sending: the connecting IP address, its reverse DNS name, the domain's published sending rules, and a cryptographic signature on the message. It then decides whether to accept the message and, if so, whether it belongs in the inbox or the spam folder.
5. The recipient's app picks the message up over IMAP or JMAP.

If step 3 or 4 fails temporarily — the receiving server is down, or it asks your server to try later — your server keeps the message in a queue and tries again, typically for several days. That is why email survives outages that would break most other services: a server that is offline for an hour misses nothing, it just receives the backlog late.

## A Short History

### An open network

Email predates the web by two decades. By the early 1980s, mail moved between the networked computers of universities, labs and companies under a simple rule: any server would accept a message from any other and, if needed, pass it along toward its destination. SMTP was written for that world. It had no way to check who a sender really was, because everyone on the network knew each other.

The design was a success. The protocol that carries mail today is still recognizably the one from 1982, and it still works between any two servers that agree to talk.

### Spam, and the first walls

When the internet opened to the public in the 1990s, that trust became a liability. Advertisers discovered that sending a million messages cost almost nothing, and servers that relayed mail for anyone carried their spam for free.

Administrators responded by building walls, one at a time:

- Servers stopped relaying mail for strangers.
- Shared **blocklists** appeared: lists of IP addresses known to send spam, which any mail server could consult before accepting a message.
- Internet providers began blocking outgoing connections on port 25 from home connections, after spammers learned to send through millions of infected home computers. Blocklists began listing whole ranges of home addresses as places mail servers shouldn't run.

### Proving who you are

In the 2000s, the industry added ways for a domain to vouch for its own mail:

- **SPF** lets a domain publish which servers may send its mail.
- **DKIM** lets a sending server sign each message, so the receiver can verify it came from the domain it claims and wasn't altered.
- **DMARC** ties the two to the address the reader sees, and tells receivers what to do with mail that fails.

These made forging a domain's mail much harder. They did not stop spam: a spammer can set up SPF and DKIM on a fresh domain as easily as anyone. Proving who you are says nothing about whether you are worth hearing from.

### Reputation decides

So receivers turned to **reputation**: a running score for every sending IP address and domain, built from how much mail it sends, how many recipients mark it as spam, how many open or reply, and whether it appears on blocklists. Mail from a sender with a good history reaches the inbox; mail from a sender with a bad or empty history gets filtered or refused.

Reputation works best at scale. A provider that sees billions of messages a day can score senders accurately, so users gravitated to the providers with the best filtering, and those providers' view of who is trustworthy became the one that matters. Today most of the world's personal mailboxes sit with a few companies — Google, Microsoft, Apple and Yahoo among the largest — and they set the rules everyone else sends by. Since 2024, Gmail and Yahoo require every sender to authenticate its mail and hold bulk senders to stricter rules still; Microsoft followed for Outlook.com in 2025.

### Where that leaves us

The protocol is still open. Any server can still receive mail from anyone, and receiving your own mail on your own hardware works well. **Sending** is the hard part: a new sender has no reputation, and a sender from an address range with a bad history starts below zero. Mail from a server that is configured perfectly can still land in spam or bounce, for reasons the sender cannot see or fix alone. [Deliverability](deliverability.md) explains how that plays out, and what to do about it.
