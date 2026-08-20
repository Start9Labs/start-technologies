# Third-party notices

Everything in this repository is MIT (see [LICENSE](LICENSE)) except the files
listed here, which carry their own terms. If you find something that belongs on
this list, please open an issue.

Third-party code pulled in at build time (Cargo and npm dependencies, the Debian
base of the StartOS image, OpenWrt itself) is out of scope here; it keeps its own
license and is not redistributed in this repository.

## Not MIT

### `projects/start-wrt/openwrt-overlay/`, `projects/start-wrt/openwrt-patches/`

GPL-licensed, mostly GPL-2.0-only. Files added to, and patches against, the
OpenWrt build tree — copyright SpaceMiT Ltd., OpenWrt.org and the Linux kernel
contributors. Sixteen carry an explicit `SPDX-License-Identifier` header. Full
text: [`projects/start-wrt/openwrt-overlay/COPYING`](projects/start-wrt/openwrt-overlay/COPYING).
Details: [`projects/start-wrt/openwrt-overlay/README.md`](projects/start-wrt/openwrt-overlay/README.md).

### `projects/start-os/docs/src/assets/firmware/binaries/Intel_ATJSLCPX-AT0043.cap`

Proprietary. Intel's final BIOS release for the NUC11ATKC4, redistributed so
owners of the discontinued 2023 Server One can still flash it. Copyright (c)
2000-2015 Intel Corporation, all rights reserved; governed by Intel's own
firmware license, which permits neither decompilation nor sublicensing. Start9
grants no rights to it.

### `projects/start-os/build/image-recipe/raspberrypi/squashfs/usr/bin/extract-ikconfig`

GPL-2.0. The Linux kernel's `scripts/extract-ikconfig`, shipped unmodified in the
Raspberry Pi image; it extracts a kernel's embedded `.config`. Copyright (c)
2009, 2010 Dick Streefland. Its own header carries the notice.

### `shared-libs/ts-modules/shared/assets/fonts/Hanken_Grotesk/`, `projects/start-wrt/web/assets/fonts/Hanken_Grotesk/`

SIL Open Font License 1.1. Copyright 2021 The Hanken Grotesk Project Authors
(<https://github.com/marcologous/hanken-grotesk>). Full text: `OFL.txt` beside
the font files in each directory.

### `.github/actions/upload-each/dist/index.js`

A committed `ncc` bundle inlining this action's npm dependencies — 97 packages,
of which 76 are MIT, 9 ISC, 7 Apache-2.0, 2 BlueOak-1.0.0, and one each
0BSD, BSD-2-Clause and `Apache-2.0 AND BSD-3-Clause`. The
per-dependency notices are generated alongside it as
[`.github/actions/upload-each/dist/index.js.LICENSES.txt`](.github/actions/upload-each/dist/index.js.LICENSES.txt);
regenerate both together with `npm run build` in that directory.

## MIT, but not ours alone

These are MIT — the repository's blanket claim holds — but they are derived from
someone else's work and carry that author's copyright as well as Start9's.

| Path                                                                                          | Derived from                                                          | Upstream copyright                                                        |
| --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `shared-libs/crates/imbl-value/` (`de.rs`, `ser.rs`, `index.rs`, `macros.rs`)                 | [serde_json](https://github.com/serde-rs/json)                        | Erick Tryzelaar and David Tolnay (MIT OR Apache-2.0, used here under MIT) |
| `shared-libs/crates/patch-db/json-patch/`                                                     | [idubrov/json-patch](https://github.com/idubrov/json-patch)           | Ivan Dubrov (MIT OR Apache-2.0)                                           |
| `shared-libs/crates/jsonpath/`                                                                | [freestrings/jsonpath](https://github.com/freestrings/jsonpath)       | Changseok Han (MIT)                                                       |
| `projects/start-os/build/image-recipe/raspberrypi/img/usr/lib/startos/scripts/init_resize.sh` | [RPi-Distro/raspi-config](https://github.com/RPi-Distro/raspi-config) | Alex Bradbury (MIT)                                                       |

## Trademarks

Product names and logos appearing in this repository — Bitcoin Core, LND, Let's
Encrypt and the service brands shown in the marketplace — are the trademarks of
their respective owners and are used for identification only. No trademark
rights are granted by the MIT license.

## Key material

`projects/start-wrt/openwrt-overlay/` carries SpaceMiT's opkg signing keys and a
vendor root CA certificate, shipped unmodified from the vendor BSP. They are key
material rather than code, and are listed here for provenance.
