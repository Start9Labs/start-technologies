# Flashing Firmware - Server One (2023)

Start9's 2023 Server One was the Intel NUC11ATKC4, whose BIOS was refered to as "ATJSLCPX" by Intel, and whose latest release was AT0043.cap before they officially discontinued support for the product line.

> [!NOTE]
> The BIOS capsule linked below is Intel's, not Start9's — Copyright (c) Intel
> Corporation, all rights reserved, and subject to Intel's own firmware license.
> We mirror it unmodified because Intel withdrew it when the product line was
> discontinued. It is not covered by StartOS's MIT license.

## Required Equipment

- A monitor and keyboard.
- A USB stick, formatted FAT32.

## BIOS Update Steps

1. Download [Intel_ATJSLCPX-AT0043.cap](assets/firmware/binaries/Intel_ATJSLCPX-AT0043.cap) to the USB stick

   > [!TIP]
   > If you wish to confirm the integrity of your download before you flash it, here is the sha256sum:
   >
   > `e72c356cdefa90486c031b7bd3d616cfd4e34e76dffb9f3ba72928355db3185b  Intel_ATJSLCPX-AT0043.cap`

1. Insert the power cable, video cable, keyboard, mouse, and USB stick with the ATJSLCPX BIOS into the Server One.

1. Power the unit on and continually press `F7` on your keyboard to launch the BIOS update screen.

1. Press enter 3 times to update the BIOS by selecting and confirming your intention to flash `Intel_ATJSLCPX-AT0043.cap` from the USB stick.

1. Power the unit off when it reboots, and remove the BIOS USB stick.

1. Power the unit on and continually press `F2` to enter the bios settings.

1. Arrow up, then right to the `Power` menu (near the top right).

1. Arrow to `Secondary Power Settings` at the bottom.

1. Arrow down to `After Power Failure` and set the value to "Power On".

1. Arrow to `Wake on LAN from S4/S5` and set the value to "Stay Off".

1. Arrow up, then over to the `Boot` menu (top right).

1. Arrow down to `Boot Priority`.

1. Set _all 4_ `UEFI PXE & HTTP Network` boot options to "Disabled".

1. Arrow down to `Boot USB Devices First` and enable (check) it.

1. Hit `F10` to save changes and exit, followed by "Yes".
