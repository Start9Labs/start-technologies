# Installing StartOS

This guide is for flashing StartOS to a USB drive, then installing it onto a desktop, laptop, or mini PC. For an up-to-date list of known-good hardware, please check out this [forum post](https://community.start9.com/t/known-good-hardware-master-list-hardware-capable-of-running-startos/).

## Watch The Video

<div class="yt-video" data-id="5hGeaBzMfWQ" data-title="Installing StartOS"></div>

## Download

1.  Visit the [Github release page](https://github.com/Start9Labs/start-technologies/releases/latest) to find the latest version of StartOS.

1.  Under "ISO Downloads", select the ISO for your architecture. StartOS is available in x86_64 (AMD64), aarch64 (ARM64), and RISC-V (RVA23). For x86_64 and aarch64, two variants are available:
    - **Standard**: Includes proprietary firmware and drivers for broader hardware compatibility, including display and wireless. Recommended for most users.

    - **Slim (FOSS-only)**: 100% open source, containing **no** proprietary firmware or drivers. Only compatible with certain hardware, such as the Start9 Server Pure.

1.  Verify the SHA256 checksum against the one listed on GitHub (optional but recommended).
    - **Mac**. Open a terminal and run:

          openssl dgst -sha256 <filename>.iso

    - **Linux**. Open a terminal and run:

          sha256sum <filename>.iso

    - **Windows**. Open PowerShell and run:

          Get-FileHash <filename>.iso

## Flash

1. Download and install [balenaEtcher](https://www.balena.io/etcher) onto your Linux, Mac, or Windows computer.

1. Insert your USB drive into your computer.

1. Open balenaEtcher.

1. Click "Select Image" and select the `.iso` image you just downloaded.

1. Click "Select Target" and select your microSD card.

   > [!WARNING]
   > BE ABSOLUTELY CERTAIN you have selected the correct target USB flash drive. Whatever target you select will be **COMPLETELY ERASED**!!

1. Click "Flash!". You may be asked to approve the unusually large disk target and/or enter your password. Both are normal.

## Install

1. Remove the newly-flashed USB drive from your computer and plug it into your server. Choose the fastest available USB 3.0+ port - typically this is blue or labeled "SS" (SuperSpeed).

1. Plug your server into a surge protector — never directly into the wall.

   > [!IMPORTANT]
   > A surge protector is **required** to safely operate your server. Power surges can permanently damage hardware and corrupt your data drive. An [uninterruptible power supply (UPS)](surge-and-ups.md) is strongly recommended for added protection.

1. Power on your server, booting from USB.

   > [!TIP]
   > Some devices do not automatically boot from USB. In these cases, you will need to access your device's BIOS settings and change the boot order to prioritize the USB drive. This is known to be required on the **Nvidia DGX Spark**, among others. You may also need to turn off Secure Boot or explicitly allow USB boot. See the [Community Hub](https://community.start9.com) for device-specific guides or to get help.

1. The StartOS install wizard will now be available at `http://start.local`. You can also use a monitor, keyboard, and mouse. This is known as "Kiosk Mode".

1. Select your language and press Continue.

1. At the Select Drives dialog, select the disk(s) you want to use as your OS and data drives.

1. If a previous StartOS install is detected, it will ask whether you want to Overwrite or Preserve the existing StartOS data. Select Overwrite to start fresh, or Preserve to flash the OS on the booted USB thumb drive over the old installation, while preserving your data. After install is complete, you will be prompted to Continue to Setup.
