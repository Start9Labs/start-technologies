# Hardware

Every router Start9 has sold, newest first. Each entry lists the specifications and how to tell that model apart. What is for sale now is at [store.start9.com](https://store.start9.com). For servers, see [Start9 Hardware](/start-os/start9-hardware.html) in the StartOS documentation.

## RISC-V Router

- **Sold:** October 2026 – present
- **Base hardware:** BananaPi BPI-F3
- **CPU:** SpaceMiT K1, 8-core RISC-V
- **RAM:** 4 GB LPDDR4
- **Storage:** 16 GB eMMC, plus a microSD slot for flashing
- **Ethernet:** 1× Gigabit WAN, 1× Gigabit LAN
- **Wi-Fi:** AsiaRF AW7916-NPD mini-PCIe module (MediaTek MT7916), Wi-Fi 6E (802.11ax) — 2T2R on 2.4 GHz, 2T3R on 5/6 GHz, up to 2402 Mbps; StartWRT runs it on 2.4 GHz and 5 GHz concurrently. Sold separately to US customers, as FCC regulations require
- **Ports:** 2× USB 3.0 Type-A
- **Power:** 12 V / 3 A DC; shipped with a C13/C14 mains cord
- **How to identify it:** Start9 wordmark on top, a small DeepComputing logo on the back, and a Wi-Fi password sticker on the bottom
- **Known issues:** none

The sections below publish the K1 schematic this router's board descends from, and record where the router differs from it.

### Schematic

[SpacemiT K1 reference schematic (PDF, 28 sheets)](assets/hardware/spacemit-k1-reference-schematic.pdf)

This is SpacemiT's `SPACEMIT-K1_LP4XP200_32X1` design, revision V3.0, dated April 2024. It documents the power tree, the clock and GPIO maps, the processor, memory and storage, and the peripheral interfaces the K1 supports. StartWRT boots the `k1-x_deb1` device tree, which describes this same design.

### Why It Differs From Your Router

A schematic is a design document, not a parts list for the unit on your desk. Two ordinary things put distance between the two.

**A reference design carries every option; a product populates a subset.** The schematic draws every interface the processor can drive, so that anyone building on the K1 can see how each one is wired. A finished product fits only the parts it uses. Nothing is removed from the drawing — the rest is simply never populated.

**Parts get substituted.** Regulators, transistors and passives are second-sourced routinely, and a pin-compatible replacement drops into the same footprint without anyone redrawing a sheet. Memory is the most visible case: capacity variants of one package are interchangeable, so the density printed on a schematic is not necessarily the density installed.

> [!NOTE]
> Read the schematic as documentation of the platform, not as a bill of materials for your router.

### Notable Differences

| On the schematic                                                  | On the router                                                                                                               | Why                                                                               |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Memory densities up to 16 GB                                      | 4 GB LPDDR4                                                                                                                 | Same package and ballout, so the density is a drop-in substitution.               |
| An onboard 2T2R Wi-Fi and Bluetooth radio on SDIO                 | No onboard radio. Wi-Fi comes from an AsiaRF AW7916-NPD Wi-Fi 6E module in the mini PCIe slot the schematic also documents. | A removable module carries a far stronger radio, and can be replaced or upgraded. |
| A USB 3.0 hub, a USB 2.0 Type-C port, and a barrel jack for power | 2 × USB 3.0 Type-A                                                                                                          | The reference design's port arrangement is not the one the enclosure exposes.     |

### What the Schematic Does Not Cover

The schematic shows how components connect. It is not the circuit board layout: it does not include the copper artwork or the layer stackup, nor the bill of materials naming the specific parts fitted to a production run.
