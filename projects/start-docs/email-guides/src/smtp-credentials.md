# SMTP Credentials

Get credentials for an account that can send mail — from Forward Email, Amazon SES, Gmail, Mailgun, Proton Mail or SendGrid — for StartOS, a Stalwart relay, or any app.

## What They're For

SMTP credentials let software hand mail to a provider for delivery. You need them for:

- **StartOS and its services** — Nextcloud, Vaultwarden, Gitea and others send notifications and password resets. Enter the credentials once under **System › SMTP** in StartOS, then point each service at them. See [SMTP in the StartOS docs](/start-os/smtp.html).
- **A Stalwart relay** — Stalwart hands your outgoing mail to the provider, which delivers it from its established servers. The credentials must authorize every From address the server will use; a credential for one alias cannot necessarily relay mail for the whole domain. See [Using a Relay for a Whole Domain](#using-a-relay-for-a-whole-domain) and the relay section of [Stalwart's instructions](https://github.com/Start9Labs/stalwart-startos/blob/master/instructions.md#sending-through-a-relay-provider).
- **Any other app** that sends mail, with the same values.

Every provider gives you the same handful of values: a **host**, a **port** and **connection security**, a **username** and **password**, and the **From address** you're allowed to send as.

## Choosing a Provider

We recommend [Forward Email](https://forwardemail.net): it is open source and privacy-focused, and sends from your own domain. If you already host your domain's mail there (see [Hosted Email on Your Domain](hosted-email.md)), you already have what you need.

- **Sending as your own domain** looks more trustworthy to recipients and keeps working if you change providers. Forward Email, Amazon SES, Mailgun, SendGrid and Proton all support it.
- **Gmail** is the quickest to set up if you have an account, but sends only as your Gmail address.
- **Amazon SES, Mailgun and SendGrid** are built for apps sending in volume: newsletters, a business's automated mail.

## Getting Credentials

{{#tabs global="smtp-provider"}}

{{#tab name="Amazon SES"}}

To use Amazon SES you will need:

- An Amazon Web Services (AWS) account. If you don't have one, you can [register here](https://aws.amazon.com/) for free.
- Amazon Simple Email Service (SES) set up from [inside your AWS console](https://console.aws.amazon.com/ses/), free for a time within [certain limits](https://aws.amazon.com/ses/pricing/).
- A verified identity to send from: either your own domain name (add the DNS records Amazon gives you) or a single email address, under **Identities** in the SES console.

New SES accounts start in the _sandbox_, which can only send to addresses you have also verified. To send to anyone else, request production access from the SES **Account dashboard**.

1. Open the [SES console](https://console.aws.amazon.com/ses/) and check the **region** selected in the top bar. Credentials, verified identities, and the SMTP host are all specific to this region, so use the same one throughout.

1. Choose **SMTP settings** in the left navigation pane. The page shows two cards: **Mail Manager SMTP** (marked "Recommended") and **IAM SMTP credentials**.

1. In the **IAM SMTP credentials** card, click **Create IAM credentials**. The IAM console opens. Do not use the Mail Manager card's **Create SMTP credentials** button: it creates an _ingress endpoint_, a separately billed product that is not needed.

1. Enter a user name (or keep the default) and click **Create user**.

1. Click **Show** under _SMTP password_, then **Download .csv file**. The SMTP user name and password are shown only once; save them somewhere secure, such as your Vaultwarden password manager. The user name is an access key ID (starts with `AKIA`); the password is _not_ your AWS secret access key.

1. Use these values:

   | Parameter           | Value                                                                          |
   | ------------------- | ------------------------------------------------------------------------------ |
   | Host                | `email-smtp.<region>.amazonaws.com`, e.g. `email-smtp.us-east-1.amazonaws.com` |
   | Connection Security | TLS                                                                            |
   | Port                | 465                                                                            |
   | From Address        | an address at your verified domain, or your verified email address             |
   | Username            | your SMTP user name (from above)                                               |
   | Password            | your SMTP password (from above)                                                |

   > [!NOTE]
   > Replace `<region>` in the **Host** with the region you created the credentials in, e.g. `email-smtp.eu-west-1.amazonaws.com`. The SMTP settings page in the SES console shows the exact hostname for that region. For STARTTLS instead, use port **587**.

{{#endtab}}

{{#tab name="Forward Email"}}

To use Forward Email you will need a domain name on a Forward Email paid plan, which includes outbound SMTP.

1. Follow Forward Email's [Send Email with Custom Domain](https://forwardemail.net/en/guides/send-email-with-custom-domain-smtp) guide until it has you configure an email application. By then your domain is set up for outbound SMTP and you have an alias to send from, such as `no-reply@yourdomain.com`, and its generated password.

1. Use these values:

   | Parameter           | Value                             |
   | ------------------- | --------------------------------- |
   | Host                | smtp.forwardemail.net             |
   | Connection Security | TLS                               |
   | Port                | 465                               |
   | From Address        | no-reply@yourdomain.com           |
   | Username            | no-reply@yourdomain.com           |
   | Password            | the alias's password (from above) |

   > [!NOTE]
   > The **From Address** must be the alias in **Username**; Forward Email rejects mail sent from any other address. It may optionally include a display name, e.g. `Your Name <no-reply@yourdomain.com>`.

{{#endtab}}

{{#tab name="Gmail"}}

1. Access your Google 2-step verification settings: https://myaccount.google.com/signinoptions/two-step-verification.

1. Enable 2-Step verification if not already.

1. Go to the App Passwords page: https://myaccount.google.com/apppasswords. Google no longer links to it from the 2-Step Verification page, so navigate there directly.

1. Choose a name for the new App Password. You may call it anything, such as "SMTP", then click "Create".

1. A random 16-character password will be created and shown to you. This is your app password. Save it somewhere secure, such as your Vaultwarden password manager, then click "Done".

1. Use these values:

   | Parameter           | Value                          |
   | ------------------- | ------------------------------ |
   | Host                | smtp.gmail.com                 |
   | Connection Security | TLS                            |
   | Port                | 465                            |
   | From Address        | your-username@gmail.com        |
   | Username            | your-username@gmail.com        |
   | Password            | your App Password (from above) |

   > [!NOTE]
   > For STARTTLS instead, use port **587**. The **From Address** may optionally include a display name, e.g. `Your Name <your-username@gmail.com>`.

{{#endtab}}

{{#tab name="Mailgun"}}

To use Mailgun you will need a Mailgun account with a sending domain you have verified by adding the DNS records Mailgun gives you.

1. In Mailgun, open your domain's **SMTP credentials** and create a credential, such as `no-reply@mg.yourdomain.com`. Save its password somewhere secure; Mailgun shows it only once. See Mailgun's [SMTP credentials](https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-smtp) guide.

1. Use these values:

   | Parameter           | Value                             |
   | ------------------- | --------------------------------- |
   | Host                | smtp.mailgun.org                  |
   | Connection Security | TLS                               |
   | Port                | 465                               |
   | From Address        | an address at your sending domain |
   | Username            | your SMTP credential's login      |
   | Password            | your SMTP credential's password   |

   > [!NOTE]
   > A domain in Mailgun's EU region sends through `smtp.eu.mailgun.org`.

{{#endtab}}

{{#tab name="Proton Mail"}}

Access to Proton Mail's SMTP settings is currently only made available by Proton to their customers with **Proton for Business**, as well as certain higher tier individual and family plans (**Proton Duo**, **Proton Family** – both with "SMTP Submission" as a listed feature), and then only when you point a custom domain to your account.

To set up Proton Mail for SMTP you will need:

- To purchase and point a domain name from a domain registrar to Proton's servers by following this guide here: [Custom Domain](https://proton.me/support/custom-domain)
- To follow the steps in the section **How to set up SMTP** in the guide here: [SMTP Submission](https://proton.me/support/smtp-submission)

{{#endtab}}

{{#tab name="SendGrid"}}

To use SendGrid you will need a SendGrid account with a verified sender: either a single sender address or an authenticated domain.

1. In SendGrid, create an API key with **Mail Send** permission. Save it somewhere secure; SendGrid shows it only once. See SendGrid's [SMTP integration](https://www.twilio.com/docs/sendgrid/for-developers/sending-email/integrating-with-the-smtp-api) guide.

1. Use these values:

   | Parameter           | Value                        |
   | ------------------- | ---------------------------- |
   | Host                | smtp.sendgrid.net            |
   | Connection Security | TLS                          |
   | Port                | 465                          |
   | From Address        | your verified sender address |
   | Username            | `apikey`, exactly as written |
   | Password            | your API key (from above)    |

{{#endtab}}

{{#endtabs}}

## Using a Relay for a Whole Domain

Choose credentials that authorize all of the domain's sending addresses. Forward Email's alias password above authorizes only that alias: putting it in Stalwart's default relay route rejects other users' From addresses. Forward Email offers [domain-wide catch-all passwords](https://forwardemail.net/en/email-api#create-domain-wide-catch-all-password) for SMTP sending; follow its documentation for obtaining domain-wide credentials. Alternatively, configure separate authenticated relay routes for each sending alias. Protect domain-wide credentials as secrets that can send as anyone on the domain.

Publish every DNS record the relay provider requests, including its DKIM and verification records where required. Add the provider to the domain's SPF record with the `include:` value it gives you, keeping a single SPF record. See [Your Own Domain](your-own-domain.md#the-records-mail-uses).
