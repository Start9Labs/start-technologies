# Hardware

The Start9 router is built on the SpacemiT K1, an eight-core RISC-V processor. This page publishes the K1 schematic the router's board descends from, and records where the router you own differs from it.

## Specifications

|           |                                                             |
| --------- | ----------------------------------------------------------- |
| Processor | SpacemiT K1, 8-core RISC-V                                  |
| Memory    | 4 GB LPDDR4                                                 |
| Storage   | 16 GB eMMC                                                  |
| Ethernet  | 1 × gigabit WAN, 1 × gigabit LAN                            |
| Wi-Fi     | AsiaRF AW7916-NPD, Wi-Fi 6 (802.11ax) 4T4R mini PCIe module |
| USB       | 2 × USB 3.0 Type-A                                          |

## Schematic

[SpacemiT K1 reference schematic (PDF, 28 sheets)](assets/hardware/spacemit-k1-reference-schematic.pdf)

This is SpacemiT's `SPACEMIT-K1_LP4XP200_32X1` design, revision V3.0, dated April 2024. It documents the power tree, the clock and GPIO maps, the processor, memory and storage, and the peripheral interfaces the K1 supports. StartWRT boots the `k1-x_deb1` device tree, which describes this same design.

## Why It Differs From Your Router

A schematic is a design document, not a parts list for the unit on your desk. Two ordinary things put distance between the two.

**A reference design carries every option; a product populates a subset.** The schematic draws every interface the processor can drive, so that anyone building on the K1 can see how each one is wired. A finished product fits only the parts it uses. Nothing is removed from the drawing — the rest is simply never populated.

**Parts get substituted.** Regulators, transistors and passives are second-sourced routinely, and a pin-compatible replacement drops into the same footprint without anyone redrawing a sheet. Memory is the most visible case: capacity variants of one package are interchangeable, so the density printed on a schematic is not necessarily the density installed.

> [!NOTE]
> Read the schematic as documentation of the platform, not as a bill of materials for your router.

## Notable Differences

| On the schematic                                                  | On the router                                                                                                           | Why                                                                               |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Memory densities up to 16 GB                                      | 4 GB LPDDR4                                                                                                             | Same package and ballout, so the density is a drop-in substitution.               |
| An onboard 2T2R Wi-Fi and Bluetooth radio on SDIO                 | No onboard radio. Wi-Fi comes from an AsiaRF AW7916-NPD 4T4R module in the mini PCIe slot the schematic also documents. | A removable module carries a far stronger radio, and can be replaced or upgraded. |
| A USB 3.0 hub, a USB 2.0 Type-C port, and a barrel jack for power | 2 × USB 3.0 Type-A                                                                                                      | The reference design's port arrangement is not the one the enclosure exposes.     |

## What the Schematic Does Not Cover

The schematic shows how components connect. It is not the circuit board layout: it does not include the copper artwork or the layer stackup, nor the bill of materials naming the specific parts fitted to a production run.
