# Flashing Firmware - Server Pure

This page is for the [Server Pure](start9-hardware.md) _only_ — the 2023, 2025 and 2026 models. It will not work on other devices.

Generally, you do not need to manually flash your device using this guide, as the firmware is now automatically updated on supported devices. Please only use this method if directed by a Start9 Support Technician. **If you were told to "_flash your device_", you are looking for [Installing StartOS](installing-startos.md) instead.**

## Required Equipment

- A monitor and keyboard.
- A USB stick, formatted FAT32.

## Firmware Flashing Steps

1. Power down your server if not already.

1. Connect a monitor and keyboard to your server using two of the USB3 (blue) ports.

1. Open the [latest PureBoot release](https://github.com/Start9Labs/pureboot/releases/latest) and download the `.zip` that matches your server:

   - **Most Server Pures sold since 2024:** `pureboot-librem_mini_v2-basic_usb_autoboot-start9-<version>.zip`
   - **An older Server Pure with a WiFi card:** `pureboot-librem_mini_v2-basic_usb_autoboot_blob_jail-start9-<version>.zip`

   If you are not sure which one you have, download the `basic_usb_autoboot_blob_jail` zip.

1. Extract the zip and copy the `.rom` file inside it to the USB stick.

1. Eject the USB stick and insert it into your server using a USB3 (blue) slot.

1. Turn on the server while pressing the `ESC` key on the keyboard repeatedly until you see the PureBoot Basic Boot Menu screen. Select "Options -->".

   ![step 1](assets/firmware/pure-1.jpg)

1. Select "Flash/Update the BIOS".

   ![step 2](assets/firmware/pure-2.jpg)

1. Select "Flash the firmware with a new ROM, erase settings".

   ![step 3](assets/firmware/pure-3.jpg)

1. The system will ask if you want to proceed flashing the BIOS with a new ROM, select "Yes".

   ![step 4](assets/firmware/pure-4.jpg)

1. Choose the file that we downloaded and copied to the USB stick.

   ![step 5](assets/firmware/pure-5.jpg)

1. Confirm you want to proceed with the flash by selecting "Yes".

   ![step 6](assets/firmware/pure-6.jpg)

1. The BIOS will be re-flashed with the new firmware. This may take a few minutes. When complete, remove the firmware USB, then select "OK" to complete the process.

   ![step 7](assets/firmware/pure-7.jpg)
