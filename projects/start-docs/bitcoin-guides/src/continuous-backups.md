# Continuous Backups

A continuous backup keeps a copy of a service's most volatile data current on storage you choose — Google Drive, Dropbox, a Nextcloud, or any SSH server — so that losing the server never costs you more than the last few seconds. LND and Bark Wallet ship one, because for both a StartOS backup on its own is not enough.

## Why a StartOS Backup Is Not Enough

A [StartOS backup](/start-os/backup-create.html) is taken by hand and holds each service as it was at that moment. That is fine for most services, and not for these two:

- **LND.** Your seed recovers on-chain funds only. Funds in channels come back through the **static channel backup**, `channel.backup`, which LND rewrites whenever a channel opens or closes, and which a restore uses to ask your peers to close your channels and return the funds. A StartOS backup holds that file as it was when you took it, so a channel opened since is missing from it, and its funds are not recovered.
- **Bark Wallet.** Every Ark or Lightning payment advances the wallet state, and restoring an older copy of the wallet database rolls it back past payments it has already made, which loses funds. The database is therefore left out of the StartOS backup entirely. A local copy on the server always runs, but it survives only inside a StartOS backup, so it is likely stale exactly when you need it.

A continuous backup closes that gap. The copy is encrypted before it leaves your server, so the provider only ever holds ciphertext, and a StartOS restore is what uses it: you restore as usual, and the service fetches the copy itself.

|                | LND                                                                  | Bark Wallet                                                              |
| -------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| What is copied | `channel.backup`, the static channel backup                          | The wallet database, as an encrypted snapshot                            |
| When           | Whenever your channels change, and once a day regardless             | Within seconds of every change to the wallet                             |
| Encrypted with | A key LND derives from the wallet seed                               | A key derived from the recovery phrase                                   |
| With no target | The copy inside your last StartOS backup                             | A local copy on the server, reported as a failing health check           |
| On restore     | Closes channels from every copy found, the StartOS backup's included | Loads the freshest snapshot, and refuses one older than the last it sent |

## Setting Up

Both services keep the feature under **Actions → Continuous Backups**, and both raise a task at install that points you at it.

1. Run **Configure Continuous Backups**. Fill in any combination of Google Drive, Dropbox, Nextcloud and SFTP — see [Targets](#targets) — and turn on each one's **Enabled** toggle. Entries are saved even for a target you leave disabled, and turning one off later keeps its credentials. In LND, **Forget saved credentials** removes a target for good; turn it off first.

1. Run **Back Up Channels Now** (LND) or **Back Up Now** (Bark Wallet). Both need the service running. It reports what each target said, so a typo surfaces now rather than at restore time. LND has nothing to send until your first channel has opened, which is when it creates `channel.backup`.

1. Take a StartOS backup. The targets and their credentials are stored with the service, and a StartOS backup made before you configured them does not know where the copies are.

From then on the **Continuous Backup** health check shows how long ago each enabled target last succeeded, and names any target that starts failing.

### Where the Copies Go

Each target holds a folder you name — `lnd-channel-backups` or `bark-backups` by default — and inside it a folder whose name is derived from the node's identity or the wallet's seed, so several nodes or wallets can share one account without overwriting each other. LND keeps a single `channel.backup` there, replaced on every successful upload; Bark Wallet keeps the current snapshot and a freshness marker.

> [!WARNING]
> Choose a target on a different machine. A copy kept on this same server — on its disk, or in a Nextcloud or SFTP server running on this StartOS — dies with it. LND refuses a loopback address; a folder elsewhere on the same disk cannot be told apart from a real target. Prefer two independent targets. Tor `.onion` targets are not supported yet.

## Targets

### Google Drive

Free personal accounts work. In the [Google Cloud Console](https://console.cloud.google.com/), enable the Drive API for a project and create an OAuth client ID of type **Desktop app**. Paste its Client ID and Client Secret into the form and submit once: the result contains a sign-in link. Open it, approve the access, and your browser ends on an `http://localhost` address that does not load. Copy the `code=` value from the address bar — or the whole address — into **Authorization Code** and submit again. The folder path is a folder name in your Drive root. A refresh token you already have can be pasted instead of going through the browser.

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

The directory it prints is the one the path is relative to. LND refuses a path that starts with `/` or contains `..`.

**LND checks the server's identity before sending anything.** Saving the target records the host keys the server presents and shows their fingerprints. Compare them with the fingerprint your server reports — on a Linux server, `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` — then save again with **Host key verified** turned on. Nothing is sent until you do, and the health check says so in the meantime. Changing the host or port clears the recorded key; after the server is reinstalled and its key changes, save with **Record a new host key** turned on. Bark Wallet trusts the key the server presents.

## Restoring

Restore your StartOS backup as usual — nothing has to be fetched from a provider by hand. The backup carries the targets and their credentials, and the service fetches the copies itself.

- **LND** hands the `channel.backup` inside the StartOS backup to LND, then fetches the copy from every target with saved credentials, disabled ones included, and hands those over too. Channels are recovered from all of them together, so a channel opened after the StartOS backup was taken is recovered from the target that has it. The restore waits for a target it cannot reach — the restore notice names it — and clearing that target's saved credentials in **Configure Continuous Backups** is how to stop waiting. Nothing is sent to the targets again until the restore has finished. Recovery force-closes every channel and returns the funds on-chain, and Lightning Labs strongly recommends against continuing to use a restored node: sweep the funds to another wallet, then uninstall and reinstall LND fresh.

- **Bark Wallet** fetches the freshest snapshot from your targets on the first start after the restore, decrypts it with your seed and loads it before the wallet opens, so you come back to your most recent state. If the newest copy it finds is older than the last one it sent — a target restored from an old backup, say — it refuses to load it rather than roll the wallet back, and asks for a current copy; with two targets, a rolled-back one is outvoted. With no target configured, or none reachable, the wallet starts from the seed alone: on-chain funds, plus whatever Ark balance the server's recovery mailbox can rebuild.

Two things must both be kept: your seed or recovery phrase, which decrypts every copy and cannot be recovered from anything else, and a current StartOS backup, which holds where the copies live and the credentials to fetch them.
