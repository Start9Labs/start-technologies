# SMTP

Configure StartOS with third-party SMTP credentials so that services like NextCloud, Vaultwarden, and Gitea can send email notifications. SMTP is configured under **System > SMTP**.

> [!WARNING]
> This guide is _not_ for using StartOS as an email server. It is for granting StartOS the simple ability to _send_ emails through a 3rd party SMTP server.

## Getting SMTP Credentials

The guides below are for using Gmail, Amazon SES, or Proton Mail for SMTP, but you may also use another third party provider of your choice.

{{#tabs global="smtp-provider"}}

{{#tab name="Gmail"}}

1. Access your Google 2-step verification settings: https://myaccount.google.com/signinoptions/two-step-verification.

1. Enable 2-Step verification if not already.

1. Go to the App Passwords page: https://myaccount.google.com/apppasswords. Google no longer links to it from the 2-Step Verification page, so navigate there directly.

1. Choose a name for the new App Password. You may call it anything, such as "SMTP", then click "Create".

1. A random 16-character password will be created and shown to you. This is your app password. Save it somewhere secure, such as your Vaultwarden password manager, then click "Done".

1. In StartOS, go to **System > SMTP**, select **Gmail** as the provider, and enter the values below. Choosing Gmail pre-fills the host and defaults to TLS on port 465.

   | Parameter           | Value                          |
   | ------------------- | ------------------------------ |
   | Host                | smtp.gmail.com                 |
   | Connection Security | TLS                            |
   | Port                | 465                            |
   | From Address        | your-username@gmail.com        |
   | Username            | your-username@gmail.com        |
   | Password            | your App Password (from above) |

   > [!NOTE]
   > Selecting Gmail pre-fills and locks the **Host**, and choosing **TLS** locks the **Port** at 465. To use STARTTLS instead, set **Connection Security** to **STARTTLS** and the **Port** becomes **587**. The **From Address** may optionally include a display name, e.g. `Your Name <your-username@gmail.com>`.

{{#endtab}}

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

1. In StartOS, go to **System > SMTP**, select **Amazon SES** as the provider, and enter the values below. Choosing Amazon SES pre-fills the host for `us-east-1` and defaults to TLS on port 465.

   | Parameter           | Value                                                                          |
   | ------------------- | ------------------------------------------------------------------------------ |
   | Host                | `email-smtp.<region>.amazonaws.com`, e.g. `email-smtp.us-east-1.amazonaws.com` |
   | Connection Security | TLS                                                                            |
   | Port                | 465                                                                            |
   | From Address        | an address at your verified domain, or your verified email address             |
   | Username            | your SMTP user name (from above)                                               |
   | Password            | your SMTP password (from above)                                                |

   > [!NOTE]
   > Replace `<region>` in the **Host** with the region you created the credentials in, e.g. `email-smtp.eu-west-1.amazonaws.com`. The SMTP settings page in the SES console shows the exact hostname for that region. To use STARTTLS instead, set **Connection Security** to **STARTTLS** and the **Port** becomes **587**. The **From Address** may optionally include a display name, e.g. `Your Name <you@yourdomain.com>`.

{{#endtab}}

{{#tab name="Proton Mail"}}

Access to Proton Mail's SMTP settings is currently only made available by Proton to their customers with **Proton for Business**, as well as certain higher tier individual and family plans (**Proton Duo**, **Proton Family** – both with "SMTP Submission" as a listed feature), and then only when you point a custom domain to your account.

To set up Proton Mail for SMTP you will need:

- To purchase and point a domain name from a domain registrar to Proton's servers by following this guide here: [Custom Domain](https://proton.me/support/custom-domain)
- To follow the steps in the section **How to set up SMTP** in the guide here: [SMTP Submission](https://proton.me/support/smtp-submission)

{{#endtab}}

{{#endtabs}}

## Configuring StartOS

1. Navigate to `System > SMTP`

1. Enter your SMTP credentials and hit "Save".

1. On the same page, send yourself a test email. Remember to check your spam folder. If the email goes to spam, mark it as not spam.

1. For each service you want to use the credentials to send emails, go to the dashboard of that service, click "Actions", and locate the relevant action.
