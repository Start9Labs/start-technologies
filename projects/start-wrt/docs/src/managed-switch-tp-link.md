# Managed Switch Notes: TP-Link Easy Smart

These notes cover what you need, beyond TP-Link's own manual, to set up a TP-Link Easy Smart switch for StartWRT. Follow the steps in [Connecting a Managed Switch](managed-switch.md#the-switch-side) and keep this page open beside them:

- [Before You Start](#before-you-start): reaching the switch, its first login, and getting back in if its settings page stops answering.
- [Where Each Step Lives](#where-each-step-lives): the page on the switch for each step of the guide.
- [Non-Obvious Behavior](#non-obvious-behavior): the ways this firmware ignores, undoes or misapplies a change.

They were checked on a **TL-SG208E** (hardware 1.0, firmware 1.0.1) and an **ES208G** (hardware 1.0, firmware 1.0.2), which have the same settings pages. `System Info > System Summary` shows your hardware and firmware versions. Other models and older firmware arrange their menus differently; see [TP-Link's user guide](https://www.tp-link.com/support/download/tl-sg208e/) for your model.

## Before You Start

- With the switch on its own, not connected to the router, it answers at `http://192.168.0.1`. Type the `http://` yourself: the switch has no HTTPS, and some browsers quietly turn the address into one that fails.
- A new TL-SG208E has no password. The first page asks you to create the administrator account, so do this before the switch is connected to anything else.
- If the settings page stops answering partway through, unplug the switch's power for ten seconds, plug it back in, and wait two minutes before opening the page again. The switch comes back exactly as it was at the last **Save**.
- If that fails, factory-reset the switch: hold the reset button for 10 seconds. It comes back at `http://192.168.0.1` with no settings.

## Where Each Step Lives

| Step in the guide           | Page on the switch                                                                                                                                                                    |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Password                    | `System Info > User Account`                                                                                                                                                          |
| Fixed address, DHCP off     | `System Info > IP Settings`. Set **DHCP Settings** to Disable and apply first: the address fields are locked while DHCP is on.                                                        |
| Cloud management off        | `Controller Settings`: **Cloud-Based Controller Management** off. It is off from the factory.                                                                                         |
| Management VLAN             | `VLAN > 802.1Q VLAN`, **Management VLAN** tab: leave it at 1.                                                                                                                         |
| Turn on 802.1Q VLANs        | `VLAN > 802.1Q VLAN`, **VLAN Config** tab: Enable, apply, and confirm.                                                                                                                |
| Create and edit VLANs       | **VLAN Config** tab: enter the VLAN ID and a name (up to 10 letters, digits, `-` or `_`), set each port to Untagged, Tagged or Non-member, and **Add/Edit**, not the top Apply.       |
| PVIDs                       | **Port Config** tab: select one port, enter its PVID, set **Ingress Checking** to Enable, and press the **Apply** below that form. See [Non-Obvious Behavior](#non-obvious-behavior). |
| Disable unused ports        | `Switching > Port Settings`: **State** Disable. **Actual** shows Link Down for any empty port, which is normal.                                                                       |
| Loop protection             | `Monitoring > Loop Detection`: **Loop Prevention State** Enable. It is off from the factory on the TL-SG208E.                                                                         |
| Save                        | The **Save** button at the top right of every page. It answers "Save configuration success".                                                                                          |
| Backup                      | `System Tools > Backup Restore`                                                                                                                                                       |
| Factory reset from the page | `System Tools > System Reset`                                                                                                                                                         |

## Non-Obvious Behavior

These are the ways this firmware ignores, undoes or misapplies a change, mostly without telling you.

- **Apply is not Save.** Apply changes the running switch only. A power cut or reboot brings back the last saved settings, which makes a power cycle the undo for anything not yet saved. Press **Save** once the tables read back as planned.
- **On DHCP, the switch can take the router's address.** If DHCP is on and nothing answers when the switch starts, it uses `192.168.0.1` whatever the address fields say. That is the router's Admin address, which is why the guide turns DHCP off.
- **Both 802.1Q VLAN tabs have an extra Apply at the top.** It belongs to **802.1Q VLAN enabled** and only turns 802.1Q on or off. On the **Port Config** tab, pressed after filling in a port's PVID, it answers "Configuration no change" and the PVID stays as it was: use the Apply below the form. On the **VLAN Config** tab, pressed after editing a VLAN, it reports that 802.1Q is still enabled and the VLAN stays as it was: use **Add/Edit**.
- **The Port Config tab's port list keeps earlier selections.** It shows four ports at a time, so scroll to reach ports 5 to 8. Click the port you want rather than moving with the arrow keys, and check that it is the only one highlighted before you press Apply: a port still selected from an earlier change gets the new PVID too.
- **Deleting a VLAN that is some port's PVID resets that port's PVID to 1**, which makes it an Admin port. Move the PVID first, and read the PVID table after any delete.
- **A port removed from every VLAN is put back in VLAN 1**, and VLAN 1 cannot be deleted. Create the other VLANs before you take ports out of VLAN 1.
- **A port moved to another VLAN can turn up untagged in VLAN 1 as well**, which makes it an Admin port too. On a TL-SG208E this happened twice to a port moved between profiles, once even though the port joined its new VLAN before it left the old one. After moving a port, read the VLAN 1 row: it should list only the uplink, any downlinks and the Admin ports. Set any other port there to Non-member and press **Add/Edit**, from a computer on a different port.
- **Entering an existing VLAN ID replaces that VLAN's membership** rather than adding to it. This is how you edit a VLAN, and also how you accidentally wipe one.
- **Logging in can fail for about a minute after the switch restarts.** Wait and try again.
- **The backup file is readable.** It holds the switch's address and the administrator password in plain text, and it belongs to that model: never load it into a different switch.
- **The settings page answers on every VLAN.** The ES208G answered a computer on any port that gave itself an Admin-subnet address. The router keeps other profiles out of the Admin subnet, but the password is what guards the switch.
