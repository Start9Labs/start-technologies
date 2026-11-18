# StartOS Image Recipes

`live-build` assembles the StartOS live and installer images inside the Debian
Docker builder defined here. Run the owning recipes from the repository root:

```bash
make start-os-iso PLATFORM=x86_64
make start-os-img PLATFORM=raspberrypi
```

See [StartOS build prerequisites](../../AGENTS.md#prerequisites) for Docker,
architecture emulation and image tooling. `run-local-build.sh` is the recipe's
container wrapper and accepts the prepared StartOS `.deb` path.

Generic x86/ARM images use the signed-kernel bridge pinned in
[`kernel-backports.conf`](kernel-backports.conf). Authenticated Debian APT supplies
the forky package closure from the pinned Debian snapshot. Historical snapshot
metadata has its expiry check disabled; Debian signatures and package hashes
remain required. The signed kernel and module payload archives stay
byte-identical. The forky ABI, kernel configuration and matching headers stay
together; Debian control metadata and the headers' `.kernelvariables` compiler
selector use trixie's compiler. The changed selector's package checksum is
refreshed.

`live-build` binds the generated flat APT archive through its chroot-only archive
facility. APT installs the kernel packages it needs; headers remain available for
the NVIDIA build without adding them to every image. `trusted=yes` applies only
to that build-local file archive, after the source packages passed Debian's
archive verification. No forky network source is added to the assembled OS.
After chroot assembly, `verify-kernel-backports.sh` checks the installed kernel
version, boot-binary identity and incoming GRUB preference. Retained boot files
must not displace the image's kernel when returning to the lower `+deb13` ABI.

Raspberry Pi and other vendor kernels, and RISC-V kernel selection, are unchanged.
Both Pi `rpi-v8` and `rpi-2712` packages must remain available. See
[kernel bridge maintenance](../../AGENTS.md#kernel-backport-packages) for builds
and retirement, and the root
[kernel-security review requirements](../../../../AGENTS.md#releases) for release
validation.
