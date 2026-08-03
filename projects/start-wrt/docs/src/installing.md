# Installing StartWRT

StartWRT comes pre-installed on Start9 routers. If you need to reinstall or flash a new device, follow the instructions below to create a bootable microSD card and flash the firmware.

> [!NOTE]
> StartWRT builds are currently available for RISC-V (`riscv64`) only — specifically the SpaceMiT K1 SoC used in the BananaPi BPI-F3. Builds for other CPU architectures are not yet available.

## Requirements

- A Start9 router (BananaPi BPI-F3)
- A microSD card (4 GB or larger)
- A computer to write the image

## Download the Image

1.  Download the latest StartWRT firmware image from the [Start9 releases page](https://github.com/Start9Labs/start-technologies/releases?q=start-wrt&expanded=true) (StartWRT releases are the ones tagged `start-wrt/v…`). For a fresh install, download the **sdcard** image — it is named `startwrt-<version>-<git hash>_spacemit-k1-sdcard.img.gz` (the `…-sysupgrade.img.gz` file is the [update](updating.md) payload). There is no need to decompress it — balenaEtcher flashes the `.img.gz` directly. The commands below use `startwrt.img.gz` as a placeholder for the downloaded filename.

1.  Verify the SHA256 checksum against the one listed on GitHub (optional but recommended).
    - **Mac**. Open a terminal and run:

          openssl dgst -sha256 startwrt.img.gz

    - **Linux**. Open a terminal and run:

          sha256sum startwrt.img.gz

    - **Windows**. Open PowerShell and run:

          Get-FileHash startwrt.img.gz

## Write the Image to microSD

1. Download and install [balenaEtcher](https://www.balena.io/etcher) onto your Linux, Mac, or Windows computer.

1. Insert the microSD card into your computer.

1. Open balenaEtcher, click "Select Image", and select the StartWRT image you just downloaded.

1. Click "Select Target" and select your microSD card.

   > [!WARNING]
   > BE ABSOLUTELY CERTAIN you have selected the correct target drive. Whatever target you select will be **COMPLETELY ERASED**!!

1. Click "Flash!". You may be asked to approve the unusually large disk target and/or enter your password. Both are normal.

## Flash the Firmware

1. Power off the router.

1. Insert the microSD card into the router.

1. Power on the router. It will boot from the microSD card automatically.

1. Connect to the `StartWRT` Wi-Fi network using the Wi-Fi password printed on the sticker on the bottom of the router.

1. A captive portal will open automatically. If it does not, open a browser and navigate to `router.lan`.

1. The setup wizard will guide you through the rest. If the router already has firmware installed, you can choose **Keep settings** or **Fresh Start** (full wipe). On a new device with no existing firmware, the welcome screen still appears, but **Fresh Start** is the only option offered. See [Factory Reset](factory-reset.md#reflash-microsd) for a full walkthrough of the reflash wizard.

1. When the wizard completes, power off the router, remove the microSD card, and power it back on.

## DIY and Unprogrammed Boards

Start9 routers ship with a unique Wi-Fi password programmed into the device's EEPROM and printed on a sticker on the bottom. A vendor-programmed board "just works": flash the image, boot, and connect to the `StartWRT` network with the sticker password.

If you are flashing a bare BananaPi BPI-F3 that was never programmed with a Wi-Fi password, the Wi-Fi access point will **not** come up after boot. To bring it online:

1. Connect a computer to one of the router's LAN ports with an Ethernet cable.

1. Complete [initial setup](initial-setup.md): a captive portal opens automatically (if it does not, navigate to `router.lan`) and prompts you to create your admin password. When it finishes, you are logged in to the web interface.

1. Navigate to `Points of Entry > Wi-Fi > Passwords` and click "Add".

1. Configure the password:
   - **Label** — A descriptive name (e.g. "Default").
   - **Password** — Click "Generate" to create a strong random password, or enter your own (8–63 characters).
   - **Security Profile** — Select **Admin**. Adding the first Admin password switches the Wi-Fi radios on automatically. A first password mapped to another profile is also valid — the radios just aren't switched on for you; enable them under `Points of Entry > Wi-Fi > Settings` with the "Enable Wi-Fi" toggle.

1. Click "Save". The `StartWRT` Wi-Fi network comes up — connect to it with your new password, and record the password somewhere safe.

The password lives in the router's configuration, not the EEPROM. A soft [factory reset](factory-reset.md) re-reads the (empty) EEPROM, so on an unprogrammed board Wi-Fi is down again after a reset — repeat the steps above over Ethernet to bring it back.

## Next Steps

- [Initial Setup](initial-setup.md) — Set up your admin password and configure the router
