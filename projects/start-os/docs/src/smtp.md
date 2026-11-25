# SMTP

Configure StartOS with third-party SMTP credentials so that services like Nextcloud, Vaultwarden, and Gitea can send email notifications. SMTP is configured under **System > SMTP**.

> [!WARNING]
> This guide is _not_ for using StartOS as an email server. It is for granting StartOS the simple ability to _send_ emails through a 3rd party SMTP server. To run your own mail server, see [Self-Hosting with Stalwart](/email-guides/self-hosting.html).

## Getting SMTP Credentials

You need credentials from a provider that will send mail for you. [SMTP Credentials](/email-guides/smtp-credentials.html) in the Email Guides walks through getting them from Forward Email (which we recommend), Amazon SES, Gmail, Mailgun, Proton Mail and SendGrid.

## Configuring StartOS

1. Navigate to `System > SMTP`.

1. Select your provider, or **Other** for any provider not listed, and enter your credentials. Choosing a listed provider pre-fills its host and defaults to TLS on port 465; to use STARTTLS instead, set **Connection Security** to **STARTTLS** and the **Port** becomes **587**. The **From Address** may optionally include a display name, e.g. `Your Name <you@yourdomain.com>`.

1. Hit "Save". The saved password is never shown again: the page notes that one is saved, and leaving **Password** empty keeps it when you save or send a test email. **Remove saved password** clears it.

1. On the same page, send yourself a test email. Remember to check your spam folder. If the email goes to spam, mark it as not spam.

1. For each service you want to use the credentials to send emails, go to the dashboard of that service, click "Actions", and locate the relevant action.
