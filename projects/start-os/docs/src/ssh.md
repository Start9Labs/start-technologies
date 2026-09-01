# SSH

SSH opens a command-line session on your StartOS server from a terminal on your computer. Once connected, commands entered in that terminal run on the server.

> [!WARNING]
> Accessing your server via SSH is considered advanced. Please use caution, you can cause permanent damage to your server, potentially resulting in loss of data.

## Watch The Video

<div class="yt-video" data-id="qi5H_JzcRVk" data-title="SSH"></div>

## User and privileges

The SSH user is `start9`, not `root`. Root login is disabled. The `start9` user has `sudo` privileges, so commands requiring root should use `sudo`. There is no need to run `sudo -i` or `sudo su`.

## Connect with your StartOS master password

Open Terminal on Mac or Linux, or PowerShell on Windows. Run:

```sh
ssh start9@SERVER-HOSTNAME
```

Replace `SERVER-HOSTNAME` with your server's `your-server-name.local` address.

The first time you connect, SSH displays a message like this:

```
The authenticity of host 'your-server-name.local (192.168.1.175)' can't be established.
ED25519 key fingerprint is SHA256:BgYhzyIDbshm3annI1cfySd8C4/lh6Gfk2Oi3FdIVAa.
This key is not known by any other names.
Are you sure you want to continue connecting (yes/no/[fingerprint])?
```

Confirm that the message names your server, then type `yes` and press Enter.

At the password prompt, enter the StartOS master password you use to sign in to the StartOS web interface, not your computer's login password. The terminal displays nothing while you type the password. Press Enter when finished.

Once connected, the command prompt typically begins with `start9@` followed by your server's hostname.

If `Connection closed` appears before this prompt, you are not connected. Check the server hostname, confirm that its StartOS web interface is reachable from the same computer, and try again. If it continues, copy the complete command and output and [contact Start9 support](https://start9.com/contact).

## Connect with an SSH key

### Create an SSH key

If you do not already have an SSH key pair, open a terminal and run:

```sh
ssh-keygen -t ed25519
```

Press Enter to accept the default file location. You can optionally create a passphrase to protect the SSH key on your computer. This key passphrase is different from your StartOS master password.

### Add your key to StartOS

Display your public key:

Mac or Linux:

```sh
cat ~/.ssh/id_ed25519.pub
```

Windows PowerShell:

```powershell
Get-Content $HOME\.ssh\id_ed25519.pub
```

Copy the complete key. In the StartOS web interface, go to `System > SSH`, select **Add Key**, paste the key, and select **Save**.

Open a terminal and connect:

```sh
ssh start9@SERVER-HOSTNAME
```

If prompted for `Enter passphrase for key`, enter the SSH key passphrase, not your StartOS master password.
