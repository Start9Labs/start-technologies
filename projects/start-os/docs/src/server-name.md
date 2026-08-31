# Server Name

Your server name is its [mDNS address](mdns.md) without the `.local` on the end. A server named `my-cool-server` answers to `my-cool-server.local` on your local network, and shows `my-cool-server` in your browser tab. It is also the name the server calls itself over SSH and in its own logs.

You choose the name during [initial setup](initial-setup.md), and you can change it at any time under `System > General Settings > Server Name`. A name may contain lowercase letters, numbers, and hyphens, and may be up to 32 characters long. It cannot start or end with a hyphen.

> [!WARNING]
> If you are currently connected via your mDNS address, changing the server name will require you to switch to the new address. You will be prompted with the new address after saving.
