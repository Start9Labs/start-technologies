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

1. When the wizard completes, power off the router, remove the microSD card, and power it back on. The router now boots StartWRT from its internal storage — the wizard installs everything needed for that, including the low-level boot firmware, so this works on any BananaPi BPI-F3 regardless of what (if anything) the factory installed.

## DIY and Unprogrammed Boards

Flashing a bare BananaPi BPI-F3 works with the exact steps above: the setup wizard provisions the board's internal boot firmware (the eMMC boot partitions) as part of the flash, so the router boots from internal storage after the microSD card is removed even if the board arrived with no firmware at all.

### Wi-Fi Card

The BPI-F3 has no onboard Wi-Fi — the radio comes from a mini PCIe module, and the firmware image only carries drivers for the MediaTek MT7915/MT7916 family:

- **MT7916-based modules** — the AsiaRF **AW7916-NPD**, as shipped in Start9 routers — are fully supported: 2.4 GHz and 5 GHz broadcast concurrently.
- **MT7915-based modules** (e.g. the AsiaRF AW7915-NP1) initialize, but band-selectable cards operate **one band at a time**: with the stock configuration only the 2.4 GHz network comes up, and selecting **5 GHz** alone in Wi-Fi settings leaves no working radio.
- Modules based on any other chipset have no driver in the image and will not work.

There is one more difference on unprogrammed boards — the Wi-Fi password:

Start9 routers ship with a unique Wi-Fi password programmed into the device's EEPROM and printed on a sticker on the bottom. A vendor-programmed board "just works": flash the image, boot, and connect to the `StartWRT` network with the sticker password.

If you are flashing a bare BananaPi BPI-F3 that was never programmed with a Wi-Fi password, the Wi-Fi access point will **not** come up after boot. To bring it online:

1.  Connect to the router over Ethernet (or serial console).

1.  Set a Wi-Fi password:
    - **Random** — Generates a random 12-character password and prints it:

          startwrt-cli set-wifi-password

    - **Manual** — Prompts you to enter your own password:

          startwrt-cli set-wifi-password --manual

1.  Record the printed (or entered) password — this becomes your Wi-Fi password.

The password lives in the router's configuration. A factory reset re-reads the EEPROM, so on an unprogrammed board you will need to run `startwrt-cli set-wifi-password` again after a reset.

## Next Steps

- [Initial Setup](initial-setup.md) — Set up your admin password and configure the router
