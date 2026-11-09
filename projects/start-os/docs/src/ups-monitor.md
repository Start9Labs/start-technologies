# UPS Monitor

UPS Monitor lets StartOS monitor an uninterruptible power supply (UPS) and shut down safely when the UPS reports a low battery. It is built on Network UPS Tools (NUT), and can monitor a directly connected UPS or connect to another NUT server on the network.

## Open UPS Monitor

In the StartOS web interface, open **System > UPS Monitor** and select **Connect**. Choose whether StartOS acts as the **NUT server** or a **NUT client**, then fill in the settings for that role. To change the settings later, select the settings icon on the **UPS** card; **Disconnect**, at the bottom of the same dialog, stops monitoring and clears them. To switch roles or use a different UPS, disconnect and connect again.

## NUT server

Choose **NUT server** when the UPS's USB or serial cable is connected to this server. StartOS reads the UPS itself and can share it with other devices on the network.

StartOS searches USB for a connected UPS and shows the one it finds. If it finds more than one, choose the UPS to monitor. If it finds none, check the cable and try again.

- **Other clients can monitor** — Other devices plugged into the UPS can't see its battery level on their own. Turn this on so they can monitor it through StartOS and shut down safely when its battery runs low. Set them up as NUT clients that connect to `startos@<server address>:3493`.
- **Username/Password** — Shown when other clients can monitor. The credentials the other devices use when they connect to this server as NUT clients. They are filled in with `upsclient` and a random password; change them if you like, or select the generate button for a new password.

> [!WARNING]
> **Other clients can monitor** makes the NUT server listen on every StartOS network interface. Enable it only on a trusted network and use unique credentials.

### Connect other clients

When **Other clients can monitor** is on, the UPS Monitor page shows a **Connect other clients** card with everything a client needs: this server's LAN addresses, the port (`3493`), the UPS name (`startos`), and the username and password. Each has a copy button.

To monitor the UPS from another StartOS server, open **System > UPS Monitor** on that server, select **Connect**, choose **NUT client**, and enter the host, port, username, and password. It finds the UPS name on its own. Other devices take the same details in their NUT client settings.

> [!TIP]
> The LAN address can change if your router assigns it by DHCP. Reserve this server's address in your router so clients keep finding it.

## NUT client

Choose **NUT client** when the UPS's USB or serial cable is connected to another device that runs a NUT server, such as a NAS or router. StartOS reads the UPS status from that server over the network.

- **NUT server host** — IP address or hostname of the remote server.
- **NUT server port** — Usually `3493`.
- **Username/Password** — Credentials configured on the remote NUT server for a secondary monitor.

When you save, StartOS asks the NUT server which UPS devices it monitors. If it reports one, StartOS uses it. If it reports more than one, choose the UPS that powers this server. Another StartOS server acting as a NUT server reports its UPS as `startos`.

The remote NUT deployment needs a primary monitor that can issue the forced-shutdown signal. StartOS connects as a secondary monitor and shuts down when it receives that signal.

## Check UPS status

While a UPS is connected and this page is open, StartOS checks its status every 5 seconds:

- **UPS** — StartOS's role, the NUT server host or whether other clients can monitor, whether shutdown protection is active, the charge or runtime at which StartOS shuts down, and the UPS model.
- **Power** — Whether the UPS is on wall power, on battery, low on battery, or shutting down, and the input voltage. Alerts appear here when the UPS reports that its battery needs replacing or that it is overloaded.
- **Battery** — The charge level, whether it is charging, and the estimated runtime on battery.
- **Load** — How much of the UPS's capacity is in use, the power drawn in watts, and the most power the UPS can supply.

Select the settings icon on the **UPS** card to change the connection or to disconnect.

StartOS also checks that the shutdown monitor is running and that its credentials have monitoring permission on the UPS server. A warning appears if this check fails, even when UPS readings remain available. Correct any monitoring warning before testing shutdown protection.

If StartOS cannot read valid status data, check the USB cable. For a NUT client, also check the host, port, and credentials.

## Test safely

Before testing a power outage, confirm that the status page reports normal data without monitoring warnings. Unplug the UPS from wall power—not the StartOS power cable—and verify that `ups.status` changes from online (`OL`) to on-battery (`OB`). Restore wall power before the battery becomes low.

> [!WARNING]
> A real low-battery or forced-shutdown event can tell the UPS to cut power to its battery-backed outlets. It may turn off every device connected to that outlet group. Do not run `upsmon -c fsd` as a routine test.

For a network UPS setup, keep the router and any switch between StartOS and the NUT server on UPS power. Otherwise, StartOS may lose network access before it receives the shutdown signal.
