# openwrt-overlay

Files rsynced over an OpenWrt build tree to add the SpaceMiT K1 target. They
mirror OpenWrt's own layout, so each one lands at the same relative path inside
`openwrt/`.

## License

**This directory is GPL-licensed, not MIT.** It and
[`../openwrt-patches/`](../openwrt-patches/) are exceptions to the MIT grant in
[`../LICENSE`](../LICENSE); the repository root [`NOTICE.md`](../../../NOTICE.md)
lists every exception repo-wide. The full text is in [`COPYING`](COPYING).

Most of it comes from SpaceMiT's OpenWrt BSP and from OpenWrt itself:

| Files                                                                                                                 | Copyright                                      |
| --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| `target/linux/spacemit/`, `package/boot/{opensbi,uboot}-spacemit/` — target and boot-package makefiles, image scripts | SpaceMiT Ltd.                                  |
| `target/linux/spacemit/*/base-files/etc/board.d/*`, `.../lib/preinit/79_move_config`                                  | OpenWrt.org                                    |
| `target/linux/spacemit/patches-6.18/*`                                                                                | The respective Linux kernel contributors       |
| `package/kernel/mac80211/patches/`, `package/boot/uboot-spacemit/patches/`                                            | The respective OpenWrt and U-Boot contributors |

Sixteen files carry an explicit `SPDX-License-Identifier: GPL-2.0-only` header,
most of them naming a copyright holder. The `.patch` files are diffs against
GPL-licensed sources, so the patched result carries those sources' terms. The
remaining unheadered files are vendor BSP and OpenWrt copies, plus a few
Start9-authored additions and the SpaceMiT opkg signing keys and root CA under
`base-files/etc/` (key material, not licensed works — see the root NOTICE.md).
Start9 contributes its own additions here under GPL-2.0-only so the overlay can
be distributed as one tree.

Because StartWRT distributes firmware images built from this material, GPL-2.0
§3 obliges us to make the corresponding source available to recipients.
