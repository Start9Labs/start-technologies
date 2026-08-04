# openwrt-overlay

Files rsynced over an OpenWrt build tree to add the SpaceMiT K1 target. They
mirror OpenWrt's own layout, so each one lands at the same relative path inside
`openwrt/`.

## License

**This directory is GPL-2.0-only, not MIT.** It is the one exception to the MIT
grant in [`../LICENSE`](../LICENSE) and the repository root
[`LICENSE`](../../../LICENSE). The full text is in [`COPYING`](COPYING).

Most of it comes from SpaceMiT's OpenWrt BSP and from OpenWrt itself:

| Files                                                                                                                 | Copyright                                      |
| --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| `target/linux/spacemit/`, `package/boot/{opensbi,uboot}-spacemit/` — target and boot-package makefiles, image scripts | SpaceMiT Ltd.                                  |
| `target/linux/spacemit/*/base-files/etc/board.d/*`, `.../lib/preinit/79_move_config`                                  | OpenWrt.org                                    |
| `target/linux/spacemit/patches-6.18/*`                                                                                | The respective Linux kernel contributors       |
| `package/kernel/mac80211/patches/`, `package/boot/uboot-spacemit/patches/`                                            | The respective OpenWrt and U-Boot contributors |

Sixteen files carry an explicit `SPDX-License-Identifier: GPL-2.0-only` header
naming their copyright holder. Files without a header are patches against, or
additions to, GPL-2.0 sources and are covered by the same terms — including the
Start9-authored ones, which are derivative works of the trees they patch.

Because StartWRT distributes firmware images built from this material, GPL-2.0
§3 obliges us to make the corresponding source available to recipients.
