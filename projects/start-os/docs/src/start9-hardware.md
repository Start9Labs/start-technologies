# Start9 Hardware

Every server and router Start9 has sold, newest first: the Start9 Router, the Server One (6600H) — the only server sold today — the Server One (2026), Server Pure (2026), Server Pure (2025), Server One (2024), Server One (2023), Server Pure (2023, sold earlier as the Embassy Pro and Server Pro), and the Raspberry Pi 4 based Embassy, Embassy One and Server Lite. Each entry lists the specifications, how to tell that model from its neighbours, and whether its memory and storage can be upgraded. What is for sale now is at [store.start9.com](https://store.start9.com). For hardware Start9 did not sell, see the community [known-good hardware list](https://community.start9.com/t/known-good-hardware-master-list-hardware-capable-of-running-startos/).

## Start9 Router

- **Sold:** October 2026 – present
- **Base hardware:** BananaPi BPI-F3
- **CPU:** SpaceMiT K1, 8-core RISC-V
- **RAM:** 4 GB LPDDR4
- **Storage:** 16 GB eMMC, plus a microSD slot for flashing
- **Ethernet:** 1× Gigabit WAN, 1× Gigabit LAN
- **Wi-Fi:** AsiaRF AW7916-NPD mini-PCIe module (MediaTek MT7916), Wi-Fi 6E, 2.4 GHz + 5 GHz concurrent, up to 2402 Mbps
- **Operating system:** [StartWRT](/start-wrt/)
- **How to identify it:** Start9 wordmark on top, a small DeepComputing logo on the back, and a Wi-Fi password sticker on the bottom
- **Known issues:** none

## Server One (6600H)

- **Sold:** September 2026 – present
- **Base hardware:** GenMachine Yi6000
- **CPU:** AMD Ryzen 5 6600H, 6 cores / 12 threads, up to 4.5 GHz
- **RAM:** 16 or 32 GB LPDDR5-6400, soldered — not upgradeable
- **Memory bandwidth:** about 102 GB/s (theoretical)
- **Storage:** 2 or 4 TB NVMe SSD (Samsung 990 EVO Plus or similar), M.2 2280
- **Graphics:** AMD Radeon 660M, integrated
- **Networking:** 1× Gigabit Ethernet, no Wi-Fi
- **Ports:** 2× USB 3.0, 2× USB 2.0, 1× USB-C 3.1, 2× HDMI
- **Power:** 19 V / 5 A DC
- **Dimensions / weight:** 4.5″ × 4.2″ × 1.5″ (11.4 × 10.6 × 3.75 cm), 0.81 lb (0.37 kg)
- **Warranty:** 1 year
- **Firmware:** manufacturer BIOS
- **StartOS image:** x86_64, standard
- **How to identify it:** black chassis with a Start9 logo on top
- **Known issues:** none

## Server One (2026)

- **Sold:** December 2025 – August 2026
- **Base hardware:** GenMachine Yi6000, 6800H variant
- **CPU:** AMD Ryzen 7 6800H, 8 cores / 16 threads, up to 4.7 GHz
- **RAM:** 16 or 32 GB LPDDR5-6400, soldered — not upgradeable
- **Memory bandwidth:** about 102 GB/s (theoretical)
- **Storage:** 2 or 4 TB NVMe SSD (Samsung 990 EVO Plus or similar), M.2 2280
- **Graphics:** AMD Radeon 680M, integrated
- **Networking:** 1× Gigabit Ethernet, no Wi-Fi
- **Ports:** 2× USB 3.0, 2× USB 2.0, 1× USB-C 3.1, 2× HDMI
- **Power:** 19 V / 5 A DC
- **Dimensions / weight:** 4.5″ × 4.2″ × 1.5″ (11.4 × 10.6 × 3.75 cm), 0.81 lb (0.37 kg)
- **Warranty:** 1 year
- **Firmware:** manufacturer BIOS
- **StartOS image:** x86_64, standard
- **How to identify it:** black chassis with no logo on top; otherwise identical to the Server One (6600H)
- **Known issues:**
  - The peg that holds the M.2 SSD in place can snap under pressure, loosening the SSD.
  - Some fans are faulty and make a loud whirring sound.
  - Some power supplies are faulty, delivering too little voltage or none at all.

## Server Pure (2026)

- **Sold:** January – July 2026
- **Base hardware:** MU01 mini PC (the same board as the Purism Librem Mini v2), i7-10710U variant
- **CPU:** Intel Core i7-10710U, 6 cores / 12 threads, up to 4.7 GHz
- **RAM:** 16 GB DDR4-3200 SO-DIMM, upgradeable — 2 slots, 64 GB max. The CPU runs memory at up to DDR4-2666, so faster modules run at 2666.
- **Memory bandwidth:** about 43 GB/s (theoretical)
- **Storage:** 2 or 4 TB NVMe SSD (Samsung 990 EVO Plus or similar), M.2 2280
- **Graphics:** Intel UHD, integrated
- **Networking:** 1× Gigabit Ethernet, no Wi-Fi
- **Ports:** 4× USB 3.0, 2× USB 2.0, 1× USB-C 3.1, HDMI 2.0, DisplayPort 1.2
- **Power:** 19 V DC, 3.2 A minimum; most units shipped with a 19 V / 4.7 A supply
- **Dimensions / weight:** 5.0″ × 5.0″ × 1.5″ (12.8 × 12.8 × 3.8 cm), 2.2 lb (1 kg)
- **Warranty:** 2 years
- **Firmware:** PureBoot — see [Flashing Firmware - Server Pure](firmware-pure.md)
- **StartOS image:** x86_64, slim
- **How to identify it:** jet-black chassis with black plastic plates on each side
- **Known issues:**
  - Some power supplies are faulty, delivering too little voltage or none at all.
  - A few motherboards have failed, either stopping the server entirely or leaving some USB ports not working.

## Server Pure (2025)

- **Sold:** January – December 2025
- **Base hardware:** MU01 mini PC (the same board as the Purism Librem Mini v2), i7-10610U variant
- **CPU:** Intel Core i7-10610U, 4 cores / 8 threads, up to 4.9 GHz
- **RAM:** 16 GB DDR4-3200 SO-DIMM, upgradeable — 2 slots, 64 GB max. The CPU runs memory at up to DDR4-2666, so faster modules run at 2666.
- **Memory bandwidth:** about 43 GB/s (theoretical)
- **Storage:** 2 or 4 TB NVMe SSD (Samsung 990 EVO Plus or similar), M.2 2280
- **Graphics:** Intel UHD, integrated
- **Networking:** 1× Gigabit Ethernet, no Wi-Fi
- **Ports:** 4× USB 3.0, 2× USB 2.0, 1× USB-C 3.1, HDMI 2.0, DisplayPort 1.2
- **Power:** 19 V DC, 3.2 A minimum; most units shipped with a 19 V / 4.7 A supply
- **Dimensions / weight:** 5.0″ × 5.0″ × 1.5″ (12.8 × 12.8 × 3.8 cm), 2.2 lb (1 kg)
- **Warranty:** 1 year
- **Firmware:** PureBoot — see [Flashing Firmware - Server Pure](firmware-pure.md)
- **StartOS image:** x86_64, slim
- **How to identify it:** green-tinted chassis with white plastic plates on each side
- **Known issues:**
  - Some power supplies are faulty, delivering too little voltage or none at all.
  - A few motherboards have failed, either stopping the server entirely or leaving some USB ports not working.

## Server One (2024)

- **Sold:** January 2024 – December 2025
- **Base hardware:** GenMachine Ren5000
- **CPU:** AMD Ryzen 7 5825U, 8 cores / 16 threads, up to 4.5 GHz
- **RAM:** 16 GB DDR4-3200 SO-DIMM, upgradeable — 2 slots, 64 GB max
- **Memory bandwidth:** about 51 GB/s (theoretical)
- **Storage:** 2 or 4 TB NVMe SSD (Samsung 990 EVO Plus or similar), M.2 2280
- **Graphics:** AMD Radeon (Vega 8), integrated
- **Networking:** 1× Gigabit Ethernet, no Wi-Fi
- **Ports:** 2× USB 3.0, 2× USB 2.0, 1× USB-C 3.1, 2× HDMI
- **Power:** 19 V / 5 A DC
- **Dimensions / weight:** 4.5″ × 4.2″ × 1.5″ (11.4 × 10.6 × 3.75 cm), 0.81 lb (0.37 kg)
- **Warranty:** 1 year
- **Firmware:** manufacturer BIOS
- **StartOS image:** x86_64, standard
- **How to identify it:** silver chassis
- **Known issues:** none

## Server One (2023)

- **Sold:** 2023 – December 2024
- **Base hardware:** Intel NUC 11 Essential, NUC11ATKC4
- **CPU:** Intel Celeron N5105, 4 cores / 4 threads, up to 2.9 GHz
- **RAM:** 16 GB as 2× 8 GB DDR4-2933 SO-DIMM, upgradeable — 2 slots, 32 GB max. Modules must be 1.2 V, non-ECC; faster modules run at 2933.
- **Memory bandwidth:** about 47 GB/s (theoretical)
- **Storage:** 1, 2 or 4 TB M.2 2280 SSD. The slot takes SATA drives only.
- **Graphics:** Intel UHD, integrated
- **Networking:** 1× Gigabit Ethernet; Wi-Fi 5 (Intel Wireless-AC 9462) and Bluetooth
- **Ports:** 4× USB 3.2 (2 front, 2 rear), 2× USB 2.0, 1× HDMI 2.0b
- **Power:** 19 V DC, 65 W supply
- **Warranty:** 1 year
- **Firmware:** Intel BIOS — see [Flashing Firmware - Server One (2023)](firmware-one-2023.md)
- **StartOS image:** x86_64, standard
- **How to identify it:** black Intel NUC 11 Essential chassis
- **Known issues:** Intel has discontinued the NUC line, and AT0043 is the last BIOS release it will publish.

## Server Pure (2023)

Sold as the Embassy Pro from November 2022, renamed the Server Pro in May 2023 and the Server Pure in 2024.

- **Sold:** November 2022 – end of 2024
- **Base hardware:** MU01 mini PC, bought from Purism as the Librem Mini v2, i7-10510U variant
- **CPU:** Intel Core i7-10510U, 4 cores / 8 threads, up to 4.9 GHz
- **RAM:** 32 GB DDR4-2400 SO-DIMM, upgradeable — 2 slots, 64 GB max
- **Memory bandwidth:** about 38 GB/s (theoretical) with the modules it shipped with
- **Storage:** 2 or 4 TB NVMe SSD, M.2 2280. The board also has an empty 2.5″ SATA bay.
- **Graphics:** Intel UHD, integrated
- **Networking:** 1× Gigabit Ethernet and Wi-Fi
- **Ports:** 4× USB 3.0, 2× USB 2.0, 1× USB-C 3.1, HDMI 2.0, DisplayPort 1.2
- **Power:** 19 V DC, 3.2 A minimum; most units shipped with a 19 V / 4.7 A supply
- **Warranty:** 1 year
- **Firmware:** PureBoot — see [Flashing Firmware - Server Pure](firmware-pure.md)
- **StartOS image:** x86_64, slim
- **How to identify it:** slightly green-tinted chassis with black plastic plates on each side
- **Known issues:** none

## Embassy, Embassy One and Server Lite

Several slightly different Raspberry Pi 4 builds, sold between 2020 and 2023 under these names.

- **Sold:** 2020 – 2023
- **Base hardware:** Raspberry Pi 4 Model B
- **RAM:** 8 GB, soldered — not upgradeable
- **Storage:** depending on the build, an SSD in an external enclosure, a standalone external SSD, an SSD inside a NASPi case, or a 128 or 256 GB microSD card alone
- **StartOS image:** Raspberry Pi — see [Installing StartOS](installing-startos.md#raspberry-pi). StartOS 0.4.0 supports the Raspberry Pi 4 only.
- **How to identify it:** a Raspberry Pi 4, in a NASPi case or a Raspberry Pi case, with or without an attached external SSD
- **Known issues:**
  - A new major version of StartOS cannot be installed over the air; the microSD card is reflashed instead.
  - Not recommended for the Bitcoin stack or other heavy workloads.
