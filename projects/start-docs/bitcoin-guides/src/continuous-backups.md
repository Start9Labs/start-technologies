# Continuous Backups

A continuous backup keeps a copy of a service's most volatile data current on storage you choose — Google Drive, Dropbox, a Nextcloud, or any SSH server — so that losing the server never costs you more than the last few seconds. LND and Bark Wallet ship one, because for both a StartOS backup on its own is not enough.

## Why a StartOS Backup Is Not Enough

A [StartOS backup](/start-os/backups.html) is taken by hand and holds each service as it was at that moment. That is fine for most services, and not for these two:

- **LND.** Your seed recovers on-chain funds only. Funds in channels come back through the **static channel backup**, `channel.backup`, which LND rewrites whenever a channel opens or closes, and which a restore uses to ask your peers to close your channels and return the funds. A StartOS backup holds that file as it was when you took it, so a channel opened since is missing from it, and its funds are not recovered.
- **Bark Wallet.** Every Ark or Lightning payment advances the wallet state, and restoring an older copy of the wallet database rolls it back past payments it has already made, which loses funds. The database is therefore left out of the StartOS backup entirely. A local copy on the server always runs, but it survives only inside a StartOS backup, so it is likely stale exactly when you need it.

A continuous backup closes that gap. The copy is encrypted before it leaves your server, so the provider only ever holds ciphertext. A StartOS restore uses the copies on storage targets automatically; LND's Email target requires you to supply the attachment yourself.

## Setting Up

Both services keep the feature under **Actions → Continuous Backups**, and both raise a task at install that points you at it.

1. Run **Configure Continuous Backups**. Fill in one or more targets — see [Targets](#targets) — and turn on each one's **Enabled** toggle. Entries are saved even for a target you leave disabled, and turning one off later keeps its credentials.

1. Run **Back Up Channels Now** (LND) or **Back Up Now** (Bark Wallet). Both need the service running. It reports what each target said, so a typo surfaces now rather than at restore time.

1. Take a StartOS backup. The targets and their credentials are stored with the service, and a StartOS backup made before you configured them does not know where the copies are.

From then on the **Continuous Backup** health check shows how long ago each enabled target last succeeded, and names any target that starts failing.

### Where the Copies Go

Each of the four storage targets below holds a folder you name — `lnd-channel-backups` or `bark-backups` by default — and inside it a folder whose name is derived from the node's identity or the wallet's seed, so several nodes or wallets can share one account without overwriting each other. LND's Email target sends an attachment instead.

> [!WARNING]
> Choose a target on a different machine. A copy kept on this same server — on its disk, or in a Nextcloud or SFTP server running on this StartOS — dies with it. A folder elsewhere on the same disk cannot be told apart from a real target, so nothing stops you choosing one. Prefer two independent targets.

## Targets

Each service's own instructions in StartOS list the targets it offers — LND can also email its channel backup — and what it does with each. The four below are common to both, and setting them up is the same.

### Google Drive

Free personal accounts work, through an app you create in your own Google Cloud project. **Publish that app before you sign in.** Every new app starts in Testing, and while it is in Testing Google ends each sign-in after 7 days, so backups to Drive stop a week after setup. Nothing in Google's setup says so. If yours stopped about a week after you set them up, that is why: publish the app, then sign in again, because a sign-in made while the app was in Testing still ends.

Publishing a new app now needs a home page, a privacy policy link and an authorized domain. Google added that requirement recently, and apps published before it were never asked for one. It does not check that the domain is yours. Without a website to point them at, sign in again before each week runs out, or use Dropbox, Nextcloud or SFTP instead.

In the [Google Cloud Console](https://console.cloud.google.com/):

1. Create a project, then search for **Google Drive API** and enable it.

1. Open **Google Auth Platform** and click **Get started**. Give the app a name and your email as the support email, choose **External** under **Audience** (Internal is only for Google Workspace organizations), add your contact email, and create it.

1. Under **Clients**, create a client of type **Desktop app**, and copy its **Client ID** and **Client Secret**. Google may show the secret only once.

1. Under **Branding**, fill in **Application home page** and **Application privacy policy link**, both full `https://` addresses, and add their domain under **Authorized domains** without `https://`. These three are newly required: Google does not mark them, but **Audience** refuses to publish until they are there. Google does not ask you to prove you own the domain. Leave **App logo** empty: a logo, or more than 10 authorized domains, makes Google require verification before it publishes.

1. Under **Audience**, click **Publish app** and confirm. **Publishing status** should then read **In production**.

Paste the Client ID and Client Secret into the form and submit once: the result contains a sign-in link. Open it and continue past Google's warning that it hasn't verified the app. Approve the access, and your browser ends on an `http://localhost` address that does not load. Copy the whole address, or just its `code=` value, into **Authorization Code** and submit again. The folder path is a folder name in your Drive root. A refresh token you already have can be pasted instead of going through the browser.

### Dropbox

Create an app in the [Dropbox App Console](https://www.dropbox.com/developers/apps) with scoped access to an **App folder** and the `files.content.read` and `files.content.write` permissions. Paste its App Key and App Secret and submit once, open the link and approve it, then paste the **authorization code Dropbox shows you** — not a generated access token — into **Authorization Code** and submit again. The folder path is inside the app's own folder.

### Nextcloud

Create an app password under **Settings → Security**. Enter the address you open Nextcloud at, which must be `https://` — its WebDAV address works too — with your username and the app password. For a Nextcloud on your own network with a self-signed certificate, turn on **Trust self-signed certificate**; anyone between your server and it could then read the app password, though the copy itself stays encrypted. The folder is created if it does not exist.

### SFTP

Any always-on SSH server: a NAS, a Raspberry Pi, a VPS. Enter the host, the port (22 unless you changed it) and the login username, then choose password or SSH key authentication.

**An SSH key must have no passphrase.** The agent runs unattended and has no way to enter one. Paste the whole private key, from `-----BEGIN OPENSSH PRIVATE KEY-----` to `-----END OPENSSH PRIVATE KEY-----`. Make a key for this purpose alone and add its public half to `~/.ssh/authorized_keys` on the server:

```
ssh-keygen -t ed25519 -N '' -f startos-backup
```

**The folder path is relative to where your SFTP login starts.** The agent asks the server which directory it is in when it connects, and creates the folder there. On an ordinary Linux server that is the user's home directory, so `lnd-channel-backups` ends up at `/home/<user>/lnd-channel-backups`. Other servers start you elsewhere — a NAS at the top of a share, a chrooted account at the top of its chroot — and the folder is created wherever that is, which can be the root of the filesystem. To find out, connect with an SFTP client and ask:

```
sftp user@host
sftp> pwd
```

The directory it prints is the one the path is relative to.

**Check the server's host key.** Saving the target shows the fingerprint of the host key the server presents. Compare it with the one your server reports — on a Linux server, `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`. From then on backups go only to a server presenting that key, so if the server is reinstalled and its key changes, they stop until you save the target again. The service's instructions say what else it asks of you before it sends anything.

## Restoring

Restore your StartOS backup as usual. The backup carries the targets and their credentials, and the service fetches the copies from storage targets itself. For LND's Email target, keep the newest attachment and follow **Restore Channels from File** in LND's instructions. What each service recovers, what it waits for, and what it refuses is in its own instructions in StartOS.

Two things must both be kept: your seed or recovery phrase, which decrypts every copy and cannot be recovered from anything else, and a current StartOS backup, which holds where the copies live and the credentials to fetch them.
