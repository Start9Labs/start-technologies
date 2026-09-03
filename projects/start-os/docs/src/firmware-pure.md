# Flashing Firmware - Server Pure

This page is for the Server Pure _only_. It will not work on other devices.

Generally, you do not need to manually flash your device using this guide, as the firmware is now automatically updated on supported devices. Please only use this method if directed by a Start9 Support Technician. **If you were told to "_flash your device_", you are looking for the StartOS flashing guides instead.**

## Required Equipment

- A monitor and keyboard.
- A USB stick, formatted FAT32.

## Firmware Flashing Steps

1. Power down your server if not already.

1. Connect a monitor and keyboard to your server using two of the USB3 (blue) ports.

1. Download the firmware: [pureboot-librem_mini_v2-basic_usb_autoboot-start9-30.1.1.zip](https://github.com/Start9Labs/pureboot/releases/download/start9-30.1.1/pureboot-librem_mini_v2-basic_usb_autoboot-start9-30.1.1.zip), from the [Start9 PureBoot releases](https://github.com/Start9Labs/pureboot/releases).

   > [!NOTE]
   > This is the same image StartOS installs automatically. It boots without a display or keyboard, boots a StartOS installer from USB when one is present, powers the server back on after a power loss, sets the processor to favor efficiency over peak speed, and on models with an Intel AX200 WiFi card supplies that card's firmware in a jail so the card works with the WiFi-free StartOS image.

1. Copy or move the zip file to the USB stick.

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
