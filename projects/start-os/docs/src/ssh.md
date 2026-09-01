# SSH

Secure Shell Protocol (SSH) opens a command-line session on your StartOS server from a terminal on your own computer. The `ssh` command starts on your computer; after it connects, the commands you enter run on your StartOS server.

> [!WARNING]
> Accessing your server via SSH is considered advanced. Please use caution, you can cause permanent damage to your server, potentially resulting in loss of data.

## Watch The Video

<div class="yt-video" data-id="qi5H_JzcRVk" data-title="SSH"></div>

## User and privileges

The SSH user is `start9`, not `root`. Root login is disabled. The `start9` user has `sudo` privileges, so commands requiring root should use `sudo`. There is no need to run `sudo -i` or `sudo su`.

## Connect with your StartOS master password

### Open a terminal

{{#tabs global="platform"}}
{{#tab name="Mac"}}

Press Command + Space, type `Terminal`, and press Enter.

{{#endtab}}
{{#tab name="Windows"}}

Press the Windows key, type `PowerShell`, and press Enter.

{{#endtab}}
{{#tab name="Linux"}}

Open a terminal.

{{#endtab}}
{{#endtabs}}

Run the following command in the terminal on your computer:

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

Confirm that the message names your StartOS server, then type `yes` and press Enter to trust the server's SSH public key.

When the terminal asks for `start9@...`'s password, enter the StartOS master password you use to sign in to the StartOS web interface. Do not enter your computer's login password. The terminal does not display characters, dots, or other feedback while you type the password. Press Enter when you finish.

### Confirm that you are connected

After authentication succeeds, the terminal displays a new command prompt, typically beginning with `start9@` followed by your server's hostname. Commands entered at this prompt run on your StartOS server.

You can confirm the session with these commands:

```sh
whoami
hostname
```

`whoami` should display `start9`, and `hostname` should display your server's hostname.

### Troubleshoot a closed connection

If `Connection closed` appears before the server's command prompt, the SSH session ended without opening a usable command line. Do not enter commands intended for StartOS at your computer's original prompt.

Confirm that you entered the correct server hostname and that its StartOS web interface is reachable from the same computer, then try the SSH command again. If the connection closes again, copy the complete command and output and [contact Start9 support](https://start9.com/contact).

## Connect with an SSH key

### Create an SSH key

If you do not already have an SSH key pair on your computer, open a terminal as described above and run:

```sh
ssh-keygen -t ed25519
```

Press Enter to accept the default file location. You can optionally create a passphrase, which protects the private SSH key stored on your computer. This key passphrase is separate from both your StartOS master password and your computer's login password.

Your public key is stored at `~/.ssh/id_ed25519.pub`.

### Add your key to StartOS

Display your public key in the terminal:

{{#tabs global="platform"}}
{{#tab name="Mac"}}

```sh
cat ~/.ssh/id_ed25519.pub
```

{{#endtab}}
{{#tab name="Windows"}}

```powershell
Get-Content $HOME\.ssh\id_ed25519.pub
```

{{#endtab}}
{{#tab name="Linux"}}

```sh
cat ~/.ssh/id_ed25519.pub
```

{{#endtab}}
{{#endtabs}}

Copy the complete key displayed in the terminal.

In the StartOS web interface, go to `System > SSH`, select **Add Key**, paste the public key, and select **Save**.

### Start the SSH connection

Open a terminal on your computer and run:

```sh
ssh start9@SERVER-HOSTNAME
```

Replace `SERVER-HOSTNAME` with your server's `your-server-name.local` address.

If the terminal asks for `Enter passphrase for key`, enter the passphrase you created for the SSH key. This prompt does not ask for your StartOS master password or your computer's login password.

Use the checks under [Confirm that you are connected](#confirm-that-you-are-connected) to verify that commands will run on your StartOS server.
