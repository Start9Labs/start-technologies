# Network UPS Tools (NUT)

Network UPS Tools (NUT) lets StartOS monitor an uninterruptible power supply (UPS) and shut down safely when the UPS reports a low battery. StartOS can monitor a directly connected UPS or connect to another NUT server on the network.

## Open Network UPS Tools

In the StartOS web interface, open **System > Network UPS Tools**. Select **Configure** to enable monitoring and choose a mode.

Disabling NUT stops monitoring but preserves your settings so you can enable it again without re-entering them.

## Direct UPS

Use **Direct UPS** when the UPS is connected directly to the StartOS server, usually by USB.

- **UPS Name** — Internal NUT name, such as `ups`.
- **Driver** — NUT driver for the UPS. Many USB models use `usbhid-ups`.
- **Device or address** — Device understood by the selected driver. Most USB models use `auto`.
- **Monitor username/password** — Local credentials StartOS uses to monitor the UPS.
- **Allow network clients** — Lets other machines monitor this UPS through StartOS.
- **Network client username/password** — Credentials remote NUT clients use when network access is enabled.
- **Shutdown delay** — Seconds StartOS waits after NUT issues its final shutdown warning before beginning shutdown.

> [!WARNING]
> **Allow network clients** makes the NUT server listen on every StartOS network interface. Enable it only on a trusted network and use unique credentials.

## Network UPS client

Use **Network UPS client** when another machine is connected to the UPS and already runs NUT as the server.

- **UPS Name** — Must match the UPS name on the remote NUT server.
- **NUT server host** — IP address or hostname of the remote server.
- **NUT server port** — Usually `3493`.
- **Monitor username/password** — Credentials configured on the remote NUT server for a secondary monitor.
- **Shutdown delay** — Seconds StartOS waits after the remote primary monitor issues its final shutdown warning before beginning shutdown.

The remote NUT deployment needs a primary monitor that can issue the forced-shutdown signal. StartOS connects as a secondary monitor and shuts down when it receives that signal.

## Check UPS status

When NUT is enabled, StartOS loads the UPS status automatically. Select **Refresh** to query it again. The page displays the configured target and every variable reported by the UPS, such as `ups.status`, `battery.charge`, `battery.runtime`, and `input.voltage`.

If StartOS cannot read valid status data, check the UPS name, host, port, credentials, driver, and USB cable or network connection.

## Test safely

Before testing a power outage, confirm that the status page reports normal data. Unplug the UPS from wall power—not the StartOS power cable—and verify that `ups.status` changes from online (`OL`) to on-battery (`OB`). Restore wall power before the battery becomes low.

> [!WARNING]
> A real low-battery or forced-shutdown event can tell the UPS to cut power to its battery-backed outlets. It may turn off every device connected to that outlet group. Do not run `upsmon -c fsd` as a routine test.

For a network UPS setup, keep the router and any switch between StartOS and the NUT server on UPS power. Otherwise, StartOS may lose network access before it receives the shutdown signal.
