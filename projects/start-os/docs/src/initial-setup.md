# Initial Setup

After [installing StartOS](installing-startos.md), follow these steps to initialize your server, set a master password, and download your Root CA.

## Watch The Video

<div class="yt-video" data-id="S5v0LW3BNj4" data-title="Initial Setup"></div>

1. Connect your server to power and Ethernet.

   > [!IMPORTANT]
   > **Always plug your server into a surge protector — never directly into the wall.** Power surges from lightning, utility events, or appliances on the same circuit can permanently damage your server's hardware and corrupt your data drive. A standard surge protector is the minimum; an [uninterruptible power supply (UPS)](surge-and-ups.md) is strongly recommended for added protection against brownouts and unexpected power loss.

1. From a computer connected to the same Local Area Network (LAN) as your server, open a browser and visit [http://start.local](http://start.local).

1. Select a setup option:
   - **Start fresh**: Select this option if you are setting up a new server. You will choose its [server name](server-name.md).

   - **Restore from Backup**: Select this option _only_ if your existing StartOS data drive has been lost or corrupted. This is for disaster recovery from a manual server backup or an automatic System checkpoint. The restored server keeps the source server's name.

   - **Transfer**: Select this option if you are transferring your existing data from one drive to another. StartOS mounts the source filesystems read-only while copying. If an ext4 filesystem must be repaired before it can be mounted, StartOS repairs it and retries. The source drive remains a usable copy if the transfer fails. The transferred server keeps the source server's name. Once the new drive is running, do not boot the old one as a server again.

1. Set a strong master password. _Make it good. Write it down_. Resetting your password is non-trivial, but your data will be preserved.

1. If you selected **Start fresh**, set your server name. This is your server's [mDNS address](mdns.md) without the `.local` on the end.

1. Once initialization completes, open your server's permanent local address, then follow the instructions for [Trusting your Root CA](./trust-ca.md) to establish a secure connection with your server.

> [!IMPORTANT]
> **If your server has Secure Boot enabled, the next restart asks you to enroll a key.** Some hardware needs a driver that Secure Boot will only load once you approve it, and the NVIDIA graphics driver is the common case. Your server generates its own key for this during setup and asks the firmware to trust it, so on the following restart you are shown a blue **MokManager** screen on the monitor — a keyboard and monitor are required, as this happens before StartOS starts.
>
> Choose **Enroll MOK → Continue → Yes**, then enter your master password. Your server then restarts and the driver loads normally. If you decline, everything else works, but hardware needing such a driver stays unavailable until you enroll the key.
