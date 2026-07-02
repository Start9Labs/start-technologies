# Wi-Fi Blackout

Wi-Fi Blackout lets you disable the Wi-Fi radio on a recurring timetable. During a blackout window, the radio is powered off entirely — all Wi-Fi devices disconnect regardless of their profile, while Ethernet-connected devices are unaffected.

## Use Cases

- **Limit late-night usage** — Disable Wi-Fi from 10 PM to 7 AM so household members are not online at night.
- **Reduce RF exposure** — Power off the radio during sleeping hours to minimize radiofrequency EMF in the home.
- **Energy savings** — Turn off Wi-Fi when nobody is home during the day.

> [!NOTE]
> Wi-Fi Blackout affects the radio hardware itself, not individual devices or profiles. When Wi-Fi is off, all Wi-Fi-connected devices are disconnected regardless of their profile. Ethernet-connected devices are unaffected. For per-profile time restrictions on Internet access (not Wi-Fi connectivity), use [WAN Blackout](security-profiles.md#wan-blackout) in Security Profiles.

## Setting a Schedule

The schedule is displayed as a 7-day visual timeline grid, with one row per day of the week. Blackout windows appear as shaded blocks on the timeline.

1. Navigate to `Points of Entry > Wi-Fi > Schedule`.

1. Click "Add" to create a blackout window.

1. Set the **start** and **end** times for the blackout period. Times use a 12-hour `HH:MM AM/PM` format, with a 15-minute quick-pick dropdown. A window may cross midnight (e.g. 10:00 PM to 6:00 AM). Setting the start time equal to the end time creates a full 24-hour window.

1. Select which **days** of the week the window applies to.

1. Click "Save".

Multiple blackout windows per day are supported. For example, you could disable Wi-Fi from 12:00 AM to 6:00 AM and again from 10:00 PM to 12:00 AM.

Overlapping windows are rejected when you save. A schedule that covers the entire week with no gap is also rejected — the system needs at least one boundary to toggle the radio on and off.

> [!TIP]
> Click a window once to edit it.

> [!NOTE]
> If the router reboots during a blackout window, the schedule is re-evaluated at boot and the radio is powered back off for the remainder of the window. There is a brief interval early in boot, before the controller starts, during which the radio may come up momentarily before the blackout is reasserted.

## Removing a Schedule

To remove a blackout window, click it and click "Delete". Removing all blackout windows effectively disables the schedule — Wi-Fi will remain on at all times.
