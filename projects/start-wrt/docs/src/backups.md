# Backups

Back up your router's configuration so you can restore it after an update, factory reset, or hardware failure. Backups capture your settings — security profiles, Wi-Fi passwords, firewall rules, VPN configurations, SSH keys, and other customizations.

## Creating a Backup

1. Navigate to `System > Settings > Backup`.

1. Click "Download Backup".

1. Enter your admin password and click "Download Backup".

1. A backup file ending in `.swbk` will be downloaded to your computer.

The backup is encrypted with your admin password. Restoring it requires the password you entered here, even if you change the admin password later.

> [!TIP]
> Create a backup before performing firmware updates. While the "Keep settings" path preserves settings, having a backup provides an extra safety net.

## Restoring a Backup

1. Navigate to `System > Settings > Backup`.

1. Under "Restore Backup", click "Choose File" and select the backup file from your computer.

1. Click "Restore".

1. A dialog explains that all current settings will be overwritten and the router will reboot. Enter the admin password in effect when the backup was created, then click "Restore & Reboot".

The router will apply the configuration and restart, with the admin password you entered. A backup from an earlier version of StartWRT is not encrypted and restores without a password.

> [!WARNING]
> Restoring a backup overwrites your current configuration entirely. Any changes made since the backup was created will be lost.

## What Is Included

| Included                         | Not Included |
| -------------------------------- | ------------ |
| Security Profiles                | Activity log |
| Wi-Fi passwords                  |              |
| Published Ports                  |              |
| Ethernet port assignments        |              |
| Inbound and Outbound VPN configs |              |
| SSH keys                         |              |
| DDNS settings                    |              |
| LAN/WAN settings                 |              |
| Admin password                   |              |
| Router name, timezone, language  |              |
| Device names you've assigned     |              |
| Reserved IPv4 addresses          |              |
| Device permissions               |              |
| Data usage history               |              |
| System logs                      |              |
