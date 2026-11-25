# Hosted Email on Your Domain

Host your own domain's mail on Forward Email and read and send it from Thunderbird on your computer and phone — the setup we recommend for most people.

## What You'll Need

- **A domain** you control. See [Your Own Domain](your-own-domain.md).
- **A Forward Email paid plan.** The free plan only forwards; storing mail and sending from your domain need a paid plan. See [Forward Email's pricing](https://forwardemail.net/en/pricing).
- **Thunderbird** on your computer, and a mail app on your phone (Thunderbird for Android, or the phone's built-in mail app).

Other hosted providers work the same way in outline: add your domain, publish the records they give you, create mailboxes, and point a mail app at their servers.

## 1. Add Your Domain to Forward Email

1. Create a Forward Email account and add your domain under **My Account › Domains**.
1. Choose a paid plan for the domain.
1. Add the DNS records Forward Email lists — the verification record, MX records, SPF, DKIM, DMARC and return path — at your DNS host, exactly as shown. Forward Email's [FAQ](https://forwardemail.net/en/faq) walks through each one, with instructions for most DNS hosts.
1. Back on the domain's page in Forward Email, verify the records.

If you already receive mail at this domain somewhere else, changing the MX records moves delivery to Forward Email. Bring your existing mail along first — see [Moving Your Mail](moving-your-mail.md).

## 2. Create Your Mailboxes

1. In Forward Email, open the domain's **Aliases** and create an alias for each address, such as `you@yourdomain.com`.
1. Click **Generate Password** next to the alias. Save the password somewhere secure, such as your Vaultwarden password manager: it is also the key to your encrypted mailbox, so losing it loses the mail stored there.
1. Request **outbound SMTP** for the domain if Forward Email hasn't enabled it yet. Forward Email approves sending for each domain, often automatically once your records verify, otherwise usually within a day.

Repeat for each person or address on the domain.

## 3. Set Up Thunderbird

1. In Thunderbird, add a new mail account with your name, address and the generated password.
1. When Thunderbird offers settings, choose **Configure manually** and enter:

   | Setting             | Incoming (IMAP)         | Outgoing (SMTP)         |
   | ------------------- | ----------------------- | ----------------------- |
   | Server              | `imap.forwardemail.net` | `smtp.forwardemail.net` |
   | Port                | 993                     | 465                     |
   | Connection security | SSL/TLS                 | SSL/TLS                 |
   | Authentication      | Normal password         | Normal password         |
   | Username            | your full address       | your full address       |

1. Click **Done**, then send yourself a test message and reply to it from another account.

Forward Email also provides calendars and contacts over CalDAV and CardDAV, which Thunderbird can sync alongside your mail. See Forward Email's [client setup](https://forwardemail.net/en/faq) for the addresses.

## 4. Set Up Your Phone

Add the same account to your phone's mail app with the same servers, ports and password. On Android, Thunderbird for Android is available from Google Play and F-Droid; on iPhone, the built-in Mail app works with the settings above.

## Sending Mail From Your Services

The same alias and password can send notifications from StartOS and its services. See the Forward Email tab in [SMTP Credentials](smtp-credentials.md). A separate alias, such as `no-reply@yourdomain.com`, keeps those messages apart from your own.
