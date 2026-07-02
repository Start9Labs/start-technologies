# SSH Access

Access your router's command line over SSH for advanced troubleshooting, package management, or direct configuration. SSH accepts both password authentication (using your admin password) and public key authentication.

> [!WARNING]
> SSH provides root access to the underlying OpenWrt system. Misconfiguration can break networking, lock you out, or require a factory reset. Only use SSH if you are comfortable with the Linux command line.

## Adding an SSH Key

1. Navigate to `System > Settings > SSH Keys`.

1. Click "Add Key".

1. Paste your public key (the contents of `~/.ssh/id_ed25519.pub` or `~/.ssh/id_rsa.pub`). The key is labeled automatically from the comment at the end of the public key (e.g. `user@hostname`).

1. Click "Save".

> [!TIP]
> If you do not have an SSH key pair, generate one:
>
> ```
> ssh-keygen -t ed25519
> ```
>
> This creates a private key (`~/.ssh/id_ed25519`) and a public key (`~/.ssh/id_ed25519.pub`). Add the public key to StartWRT. Never share the private key.

## Connecting

Once your key is added, connect from a terminal:

```
ssh root@router.lan
```

## Removing an SSH Key

1. Navigate to `System > Settings > SSH Keys`.

1. Click the trash (Delete) button on the key's row.

> [!NOTE]
> Even with no SSH keys configured, you can still connect using your admin password.
