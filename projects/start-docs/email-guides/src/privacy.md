# Email and Privacy

Who can read your mail in each kind of setup, what the metadata around it reveals, what encryption does and doesn't protect, and what it takes to stay anonymous.

## Start With Who You're Hiding From

"Private email" means different things depending on who you want to keep out:

- **Advertisers and data brokers** — companies that profile you from what you write and who you write to.
- **Your mail provider** — the company that stores your mailbox, and anyone who breaches it.
- **Governments and courts** — which can compel a provider to hand over what it holds.
- **Anyone linking your address to you** — when the goal is that your mail can't be tied to your real identity at all.

No setup protects against all four equally, and the setup that is best against one can be worse against another. Decide which matters most before choosing.

## The Limit No Setup Removes

Email between two people involves two mail providers: yours and theirs. Unless you and the recipient use end-to-end encryption, writing to someone on Gmail gives Google a readable copy, whatever you use on your end. Most of the people you write to use one of a few large providers, so ordinary unencrypted correspondence with them sits readable in those systems regardless of where you host your own mailbox.

What you control is your side: who holds **your** mailbox, your contacts and the record of everyone you correspond with.

## Content and Metadata

Every message has two parts with different exposure:

- **Content** — the body and attachments. Encryption can protect this.
- **Metadata** — who sent it, to whom, when, from which IP address, and the subject line. End-to-end encryption still exposes routing information, including sender and recipient addresses and timing. Protection for subjects and other headers depends on the encryption format and the clients or service used.

Metadata alone reveals a great deal: who you talk to, how often, and when. Treat it as no less sensitive than content.

## Encryption

**In transit.** Most mail between large providers now travels over TLS, which stops anyone on the network from reading it on the way. It protects each hop between servers, not the message itself: every server along the way decrypts and handles it in readable form.

**At rest.** Some providers encrypt stored mailboxes so their own staff can't read them. Proton encrypts incoming mail as it arrives with a key only you hold; Forward Email stores each mailbox in its own encrypted file unlocked by your password. Either way, mail from outside the service still arrives in readable form and is encrypted on arrival, so the provider handles it unencrypted for that moment.

**End to end.** Only end-to-end encryption keeps a message's content unreadable to every server between you and the recipient. It requires the recipient's cooperation:

- **Between users of the same service** — Proton to Proton, Tuta to Tuta — it is automatic.
- **With anyone else**, both sides need **OpenPGP** (built into Thunderbird, and supported by Proton) or **S/MIME**, and must exchange keys first. Few correspondents use either.

## Comparing Setups

| Setup                                            | Provider can read stored mail        | Who holds your metadata               | Ties to your identity                      |
| ------------------------------------------------ | ------------------------------------ | ------------------------------------- | ------------------------------------------ |
| Free large provider (Gmail, Outlook)             | Yes                                  | The provider                          | Account details, phone number, sign-in IPs |
| Privacy provider, its own address (Proton, Tuta) | No                                   | The provider                          | Whatever you gave at signup                |
| Your domain on a hosted provider                 | Depends on the provider's encryption | The provider                          | Your domain, plus account and payment      |
| Self-hosted on StartOS, with a relay             | Nobody but you                       | You, plus the relay for mail you send | Your domain, VPS and relay accounts        |

Self-hosting gives the most control over stored mail: no company holds your mailbox or your full correspondence history. Unless messages use end-to-end encryption, the outgoing relay and the recipient's provider handle readable content. Incoming mail is also readable by the sender's provider and your own server. Securing the server — physically and with backups you protect — becomes your job.

## Anonymity

If you need mail that can't easily be tied to your real identity, **your own domain works against you**. A domain is registered and paid for by someone, the registrar knows who, and it appears on every message you send. A VPS and a relay add more accounts and payment records. A `proton.me` or `tuta.com` address looks like millions of others.

For anonymity, use a privacy provider's own address, created over Tor or a VPN without personal details, paid for (if at all) in a way not linked to you, and used only for the purpose it was created for. Know the provider's limits: a provider can be compelled to log the IP addresses that sign in to an account, and such logs have been handed over to authorities before. Signing in only over Tor keeps your real IP address out of them.

Many people keep both: their own domain for everyday mail, and a separate anonymous address for anything sensitive.
