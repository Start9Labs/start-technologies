# Installing StartWRT

StartWRT comes pre-installed on Start9 routers. If you need to reinstall or flash a new device, follow the instructions below to create a bootable microSD card and flash the firmware.

## Requirements

- A Start9 router (BananaPi BPI-F3)
- A microSD card (4 GB or larger)
- A computer to write the image

## Download the Image

1. Download the latest StartWRT firmware image from the [Start9 releases page](https://github.com/Start9Labs/start-technologies/releases?q=startwrt&expanded=true) (StartWRT releases are the ones tagged `startwrt/v…`). For a fresh install, download the **sdcard** image — it is named `startwrt-<version>-<git hash>_spacemit-k1-sdcard.img` (the `…-sysupgrade.img.gz` file is the [update](updating.md) payload). The commands below use `startwrt.img` as a placeholder for the downloaded filename.

1. Verify the SHA256 checksum against the one listed on GitHub (optional but recommended).
   - **Mac**. Open a terminal and run:

         openssl dgst -sha256 startwrt.img

   - **Linux**. Open a terminal and run:

         sha256sum startwrt.img

   - **Windows**. Open PowerShell and run:

         Get-FileHash startwrt.img

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

1. The setup wizard will guide you through the rest. If the router already has firmware installed, you can choose **Keep settings** or **Fresh Start** (full wipe). On a new device with no existing firmware, the wizard proceeds directly to Fresh Start setup. See [Factory Reset](factory-reset.md#reflash-microsd) for a full walkthrough of the reflash wizard.

1. When the wizard completes, power off the router, remove the microSD card, and power it back on.

## DIY and Unprogrammed Boards

Start9 routers ship with a unique Wi-Fi password programmed into the device's EEPROM and printed on a sticker on the bottom. A vendor-programmed board "just works": flash the image, boot, and connect to the `StartWRT` network with the sticker password.

If you are flashing a bare BananaPi BPI-F3 that was never programmed with a Wi-Fi password, the Wi-Fi access point will **not** come up after boot. To bring it online:

1. Connect to the router over Ethernet (or serial console).

1. Set a Wi-Fi password:

   - **Random** — Generates a random 12-character password and prints it:

         startwrt-cli set-wifi-password

   - **Manual** — Prompts you to enter your own password:

         startwrt-cli set-wifi-password --manual

1. Record the printed (or entered) password — this becomes your Wi-Fi password.

The password lives in the router's configuration. A factory reset re-reads the EEPROM, so on an unprogrammed board you will need to run `startwrt-cli set-wifi-password` again after a reset.

## Next Steps

- [Initial Setup](initial-setup.md) — Set up your admin password and configure the router
