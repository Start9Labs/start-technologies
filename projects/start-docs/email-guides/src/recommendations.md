# Recommendations

What we suggest for individuals, families, small businesses, people who need anonymity, and people who want to run their own mail server.

## Most People: Your Domain, Hosted

**Register a domain and host it on Forward Email, read in Thunderbird.** You get an address you own, mail that arrives, stored mail the provider can't read, and no mail server to maintain. If you later want to self-host, or change provider, the address comes with you.

We recommend [Forward Email](https://forwardemail.net): it is open source, encrypts each mailbox with your password, and its paid plans include unlimited addresses on a domain. Proton with a custom domain is a good alternative if many of the people you write to use Proton, since mail between Proton users is end-to-end encrypted.

See [Hosted Email on Your Domain](hosted-email.md).

## Trying It Out: Forwarding First

Not ready to leave your current inbox? Register a domain and use free forwarding to send its mail to the mailbox you already have. Give out the new address as you go. When you're ready, upgrade to full hosting; nothing about the address changes.

## Families

**One domain, an address for each person, on one hosted plan.** Forward Email's paid plans include unlimited addresses and pooled storage on a domain, so a whole household fits on one plan. Each person gets their own mailbox and password, and shared addresses (`family@`, `bills@`) can forward to several people.

Children's addresses can be created and removed as they grow, and an address can outlive the person's use of it — forwarding on, rather than bouncing.

## Small Businesses

**Your business domain on a hosted provider.** For a business, mail that doesn't arrive is lost money, and mail that's down is a business that's down. Choose a provider for reliability and administration features (shared mailboxes, adding and removing staff, recovering an account when someone leaves) and keep the domain in an account the business, not an employee, controls.

Keep sending from your services and newsletters separate from staff mail: use a dedicated sending service for bulk or automated mail, so a problem there never affects your team's ability to email clients. See [SMTP Credentials](smtp-credentials.md).

We don't recommend self-hosting a business's primary mail unless someone is responsible for keeping it running.

## Anonymity

**A privacy provider's own address, created over Tor, with no domain.** Your own domain ties your mail to whoever registered and paid for it. See [Email and Privacy](privacy.md#anonymity). Keep the anonymous address strictly separate from your everyday one.

## Running It Yourself

**Stalwart on StartOS, receiving through StartTunnel and sending through a relay.** This is for people who want their mailbox, contacts and calendars on their own hardware, and are willing to look after a VPS, DNS records and a server to get it.

- Use a relay for outgoing mail. Sending directly is not worth it — see [Deliverability](deliverability.md).
- Consider a spare domain while you learn, and move your main address over once it's been running reliably for a while.
- Back up the server, and keep the backups somewhere that isn't the server.

See [Self-Hosting with Stalwart](self-hosting.md).
