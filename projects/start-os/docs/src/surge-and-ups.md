# Surge Protector and UPS

Protecting your server from power events is essential. A surge protector is the minimum requirement for safely operating a server, and an uninterruptible power supply (UPS) is strongly recommended on top of that. This page explains why both matter, what to look for, and how to set them up.

## Why a UPS matters

Servers are always-on devices, so they are exposed to every power event on your circuit: lightning surges, utility brownouts, accidental unplugging, breaker trips, appliances cycling on the same line. Any one of these can:

- **Damage hardware** — voltage spikes can destroy power supplies, drives, and mainboards.
- **Corrupt data** — an abrupt power loss while the OS is writing to disk can corrupt the data drive, sometimes irrecoverably. Lightning node databases, in particular, are sensitive to unclean shutdowns.
- **Cause unexplained reboots** — brownouts (sustained low voltage) can cause silent restarts that are difficult to diagnose.

A surge protector only handles the surge case. A UPS additionally bridges short outages, smooths out brownouts, and gives you time to shut the server down cleanly during an extended outage.

## Types of UPS

There are three common topologies. For a home server, **line-interactive** is the right choice in nearly all cases.

- **Standby (offline)** — Cheapest. Runs the load directly from wall power and switches to battery only during an outage. Acceptable for non-critical electronics but lacks voltage regulation for brownouts.
- **Line-interactive** — Recommended. Includes automatic voltage regulation (AVR) for brownouts and overvoltage, with fast switchover to battery during outages. The right balance of cost, protection, and efficiency for a home server.
- **Online (double-conversion)** — Most expensive. Continuously rebuilds the power waveform from battery. Best protection, but overkill and noticeably less efficient for a typical home server.

## What to look for

**Pure sine wave output.** Modern power supplies with active power factor correction (active PFC) — which most mini PCs and servers use — can malfunction or shut down on the "simulated sine wave" (stepped square wave) output that cheap UPSes produce. Pay extra for a pure sine wave model. This is the single most common UPS mistake.

**Sufficient VA / wattage rating.** A typical StartOS server idles at ~10–25 W and peaks around 40–60 W under load. A UPS rated for roughly 300 VA / 180 W comfortably covers it with several minutes of battery runtime. Larger UPSes give you more runtime (and headroom for a router or switch on the same unit), at the cost of price and size.

**User-replaceable battery.** UPS batteries wear out after 3–5 years. A model with a user-replaceable battery is much cheaper to maintain than one that has to be replaced outright.

**Audible alarm and status display.** You want to know when the unit is running on battery, when the battery is low, and when the battery itself needs replacing.

## How to set it up

1. Place the UPS on a hard, ventilated surface near your server. Do not enclose it — UPSes generate heat under load.

1. Plug the UPS directly into a wall outlet. Do **not** chain it through a power strip or surge protector.

1. Charge the UPS as instructed by the manufacturer before plugging in your server. Most need several hours on first charge.

1. Plug your server into one of the **battery-backed** outlets. UPSes typically have two groups of outlets: battery-backed and surge-only. Use battery-backed for the server and any networking equipment (router, modem, switch) that you want to stay up during an outage.

1. Test the UPS by briefly unplugging it from the wall while the server is running. The server should continue running on battery. Plug it back in within a minute — the goal is just to confirm it works.

## Automatic shutdown on low battery

StartOS supports automatic shutdown on low battery through [Network UPS Tools (NUT)](network-ups-tools.md). Configure NUT to monitor either a directly connected UPS, usually by USB, or a UPS served by another NUT server on the network.

If your area has frequent or long outages, size your UPS to keep your server and any required networking equipment powered long enough for StartOS to receive the low-battery signal and shut down cleanly.
